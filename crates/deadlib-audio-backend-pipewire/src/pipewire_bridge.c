#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>

/*
 * Keep PipeWire out of the executable's ELF dependency table. Rust resolves
 * these addresses with dlopen/dlsym, and this bridge uses the installed
 * headers only for ABI-safe types, constants, and static SPA pod helpers.
 */
struct ds_pipewire_api {
    uintptr_t pw_init;
    uintptr_t pw_main_loop_new;
    uintptr_t pw_main_loop_get_loop;
    uintptr_t pw_main_loop_destroy;
    uintptr_t pw_context_new;
    uintptr_t pw_context_connect;
    uintptr_t pw_context_destroy;
    uintptr_t pw_core_disconnect;
    uintptr_t pw_thread_loop_new;
    uintptr_t pw_thread_loop_get_loop;
    uintptr_t pw_thread_loop_destroy;
    uintptr_t pw_thread_loop_start;
    uintptr_t pw_thread_loop_stop;
    uintptr_t pw_thread_loop_lock;
    uintptr_t pw_thread_loop_unlock;
    uintptr_t pw_thread_loop_timed_wait;
    uintptr_t pw_thread_loop_signal;
    uintptr_t pw_properties_new;
    uintptr_t pw_properties_set;
    uintptr_t pw_stream_new_simple;
    uintptr_t pw_stream_destroy;
    uintptr_t pw_stream_connect;
    uintptr_t pw_stream_dequeue_buffer;
    uintptr_t pw_stream_queue_buffer;
};

#define DS_PW(api, name) ((__typeof__(&name))(uintptr_t)((api)->name))

typedef uint32_t (*ds_render_callback)(void *data, uint8_t *buffer,
                                      uint32_t capacity, uint32_t rate,
                                      uint32_t channels);
typedef void (*ds_error_callback)(void *data, const char *error);

struct ds_pipewire_stream {
    const struct ds_pipewire_api *api;
    struct pw_thread_loop *loop;
    struct pw_stream *stream;
    struct pw_stream_events events;
    ds_render_callback render;
    ds_error_callback report_error;
    void *callback_data;
    uint32_t rate;
    uint32_t channels;
    enum pw_stream_state state;
    char error[512];
};

static void ds_set_error(char *buffer, size_t capacity, const char *message)
{
    if (buffer == NULL || capacity == 0)
        return;
    snprintf(buffer, capacity, "%s", message != NULL ? message : "unknown PipeWire error");
}

void ds_pipewire_init(const struct ds_pipewire_api *api)
{
    DS_PW(api, pw_init)(NULL, NULL);
}

bool ds_pipewire_probe(const struct ds_pipewire_api *api)
{
    struct pw_main_loop *main_loop = DS_PW(api, pw_main_loop_new)(NULL);
    if (main_loop == NULL)
        return false;

    struct pw_context *context = DS_PW(api, pw_context_new)(
        DS_PW(api, pw_main_loop_get_loop)(main_loop), NULL, 0);
    if (context == NULL) {
        DS_PW(api, pw_main_loop_destroy)(main_loop);
        return false;
    }

    struct pw_core *core = DS_PW(api, pw_context_connect)(context, NULL, 0);
    bool available = core != NULL;
    if (core != NULL)
        DS_PW(api, pw_core_disconnect)(core);
    DS_PW(api, pw_context_destroy)(context);
    DS_PW(api, pw_main_loop_destroy)(main_loop);
    return available;
}

static void ds_state_changed(void *data, enum pw_stream_state old,
                             enum pw_stream_state state, const char *error)
{
    (void)old;
    struct ds_pipewire_stream *output = data;
    output->state = state;
    if (state == PW_STREAM_STATE_ERROR) {
        ds_set_error(output->error, sizeof(output->error), error);
        if (output->report_error != NULL)
            output->report_error(output->callback_data, output->error);
    }
    DS_PW(output->api, pw_thread_loop_signal)(output->loop, false);
}

static void ds_process(void *data)
{
    struct ds_pipewire_stream *output = data;
    struct pw_buffer *buffer = DS_PW(output->api, pw_stream_dequeue_buffer)(output->stream);
    if (buffer == NULL)
        return;

    struct spa_buffer *spa_buffer = buffer->buffer;
    if (spa_buffer != NULL && spa_buffer->n_datas > 0) {
        struct spa_data *spa_data = &spa_buffer->datas[0];
        if (spa_data->data != NULL && spa_data->chunk != NULL) {
            uint32_t written = output->render(
                output->callback_data, spa_data->data, spa_data->maxsize,
                output->rate, output->channels);
            if (written > spa_data->maxsize)
                written = spa_data->maxsize;
            spa_data->chunk->offset = 0;
            spa_data->chunk->stride = (int32_t)(output->channels * sizeof(float));
            spa_data->chunk->size = written;
        }
    }
    DS_PW(output->api, pw_stream_queue_buffer)(output->stream, buffer);
}

static void ds_pipewire_stream_cleanup(struct ds_pipewire_stream *output, bool started)
{
    if (output == NULL)
        return;
    if (started)
        DS_PW(output->api, pw_thread_loop_stop)(output->loop);
    if (output->stream != NULL)
        DS_PW(output->api, pw_stream_destroy)(output->stream);
    if (output->loop != NULL)
        DS_PW(output->api, pw_thread_loop_destroy)(output->loop);
    free(output);
}

struct ds_pipewire_stream *ds_pipewire_stream_start(
    const struct ds_pipewire_api *api, uint32_t rate, uint32_t channels,
    ds_render_callback render, ds_error_callback report_error,
    void *callback_data, char *error, size_t error_capacity)
{
    struct ds_pipewire_stream *output = calloc(1, sizeof(*output));
    if (output == NULL) {
        ds_set_error(error, error_capacity, "failed to allocate PipeWire stream state");
        return NULL;
    }
    output->api = api;
    output->render = render;
    output->report_error = report_error;
    output->callback_data = callback_data;
    output->rate = rate;
    output->channels = channels;
    output->state = PW_STREAM_STATE_UNCONNECTED;
    output->events.version = PW_VERSION_STREAM_EVENTS;
    output->events.state_changed = ds_state_changed;
    output->events.process = ds_process;

    output->loop = DS_PW(api, pw_thread_loop_new)("pipewire_out", NULL);
    if (output->loop == NULL) {
        ds_set_error(error, error_capacity, "failed to create PipeWire thread loop");
        ds_pipewire_stream_cleanup(output, false);
        return NULL;
    }

    DS_PW(api, pw_thread_loop_lock)(output->loop);
    int start_result = DS_PW(api, pw_thread_loop_start)(output->loop);
    if (start_result < 0) {
        DS_PW(api, pw_thread_loop_unlock)(output->loop);
        ds_set_error(error, error_capacity, "failed to start PipeWire thread loop");
        ds_pipewire_stream_cleanup(output, false);
        return NULL;
    }

    struct pw_properties *properties = DS_PW(api, pw_properties_new)(NULL, NULL);
    if (properties == NULL) {
        DS_PW(api, pw_thread_loop_unlock)(output->loop);
        ds_set_error(error, error_capacity, "failed to allocate PipeWire stream properties");
        ds_pipewire_stream_cleanup(output, true);
        return NULL;
    }
    DS_PW(api, pw_properties_set)(properties, PW_KEY_MEDIA_TYPE, "Audio");
    DS_PW(api, pw_properties_set)(properties, PW_KEY_MEDIA_CATEGORY, "Playback");
    DS_PW(api, pw_properties_set)(properties, PW_KEY_MEDIA_ROLE, "Music");

    output->stream = DS_PW(api, pw_stream_new_simple)(
        DS_PW(api, pw_thread_loop_get_loop)(output->loop), "audio-output",
        properties, &output->events, output);
    if (output->stream == NULL) {
        DS_PW(api, pw_thread_loop_unlock)(output->loop);
        ds_set_error(error, error_capacity, "failed to create PipeWire stream");
        ds_pipewire_stream_cleanup(output, true);
        return NULL;
    }

    uint8_t pod_buffer[1024];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(pod_buffer, sizeof(pod_buffer));
    struct spa_audio_info_raw audio_info = SPA_AUDIO_INFO_RAW_INIT(
        .format = SPA_AUDIO_FORMAT_F32,
        .rate = rate,
        .channels = channels);
    if (channels >= 2) {
        audio_info.position[0] = SPA_AUDIO_CHANNEL_FL;
        audio_info.position[1] = SPA_AUDIO_CHANNEL_FR;
    }
    const struct spa_pod *params[1];
    params[0] = spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat, &audio_info);
    if (params[0] == NULL) {
        DS_PW(api, pw_thread_loop_unlock)(output->loop);
        ds_set_error(error, error_capacity, "failed to build PipeWire audio format");
        ds_pipewire_stream_cleanup(output, true);
        return NULL;
    }

    int connect_result = DS_PW(api, pw_stream_connect)(
        output->stream, PW_DIRECTION_OUTPUT, PW_ID_ANY,
        PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS |
            PW_STREAM_FLAG_RT_PROCESS,
        params, 1);
    if (connect_result < 0) {
        DS_PW(api, pw_thread_loop_unlock)(output->loop);
        ds_set_error(error, error_capacity, "failed to connect PipeWire stream");
        ds_pipewire_stream_cleanup(output, true);
        return NULL;
    }

    while (output->state == PW_STREAM_STATE_UNCONNECTED ||
           output->state == PW_STREAM_STATE_CONNECTING) {
        int wait_result = DS_PW(api, pw_thread_loop_timed_wait)(output->loop, 5);
        if (wait_result != 0) {
            DS_PW(api, pw_thread_loop_unlock)(output->loop);
            ds_set_error(error, error_capacity, "timed out starting PipeWire stream");
            ds_pipewire_stream_cleanup(output, true);
            return NULL;
        }
    }

    if (output->state == PW_STREAM_STATE_ERROR) {
        DS_PW(api, pw_thread_loop_unlock)(output->loop);
        ds_set_error(error, error_capacity, output->error);
        ds_pipewire_stream_cleanup(output, true);
        return NULL;
    }

    DS_PW(api, pw_thread_loop_unlock)(output->loop);
    return output;
}

void ds_pipewire_stream_destroy(struct ds_pipewire_stream *output)
{
    ds_pipewire_stream_cleanup(output, true);
}
