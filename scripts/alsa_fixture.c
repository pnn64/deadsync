// Deterministic virtual device inventory for ALSA performance/regression tests.
// Open real libasound null PCMs; enumeration rate lists and queued delays are fixtures.
#define _GNU_SOURCE
#include <alsa/asoundlib.h>
#include <dlfcn.h>
#include <errno.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static int override_delay;
static snd_pcm_sframes_t fixture_delay;
static const char *fixture_layout;
static int status_error;
static int rate_matrix;
static unsigned int rate_mask;
static unsigned int rate_probe;
static int rate_override;
static const char *rate_edge;
__attribute__((constructor)) static void init_delay(void) {
    const char *delay = getenv("ALSA_FIXTURE_DELAY");
    override_delay = delay != NULL;
    if (delay) fixture_delay = strtol(delay, NULL, 10);
    fixture_layout = getenv("ALSA_FIXTURE_LAYOUT");
    status_error = getenv("ALSA_FIXTURE_STATUS_ERROR") != NULL;
    rate_matrix = getenv("ALSA_FIXTURE_RATE_MATRIX") != NULL;
    rate_edge = getenv("ALSA_FIXTURE_RATE_EDGE");
    const char *mask = getenv("ALSA_FIXTURE_RATE_MASK");
    rate_override = rate_matrix || rate_edge || mask;
    if (mask) rate_mask = strtoul(mask, NULL, 10);
}
int snd_pcm_status(snd_pcm_t *pcm, snd_pcm_status_t *status) {
    if (status_error) return -EIO;
    static int (*real_status)(snd_pcm_t *, snd_pcm_status_t *);
    if (!real_status) real_status = dlsym(RTLD_NEXT, "snd_pcm_status");
    return real_status(pcm, status);
}
int snd_pcm_delay(snd_pcm_t *pcm, snd_pcm_sframes_t *delay) {
    if (override_delay) { *delay = fixture_delay; return 0; }
    static int (*real_delay)(snd_pcm_t *, snd_pcm_sframes_t *);
    if (!real_delay) real_delay = dlsym(RTLD_NEXT, "snd_pcm_delay");
    return real_delay(pcm, delay);
}
snd_pcm_sframes_t snd_pcm_status_get_delay(const snd_pcm_status_t *status) {
    if (override_delay) return fixture_delay;
    static snd_pcm_sframes_t (*real_delay)(const snd_pcm_status_t *);
    if (!real_delay) real_delay = dlsym(RTLD_NEXT, "snd_pcm_status_get_delay");
    return real_delay(status);
}

int snd_card_next(int *card) {
    if (fixture_layout && strcmp(fixture_layout, "empty") == 0) { *card = -1; return 0; }
    *card = *card < 1 ? *card + 1 : -1;
    return 0;
}
int snd_ctl_open(snd_ctl_t **ctl, const char *name, int mode) {
    (void)mode;
    *ctl = (snd_ctl_t *)(uintptr_t)(strcmp(name, "hw:0") == 0 ? 1 : 2);
    return 0;
}
int snd_ctl_close(snd_ctl_t *ctl) { (void)ctl; return 0; }
int snd_ctl_card_info(snd_ctl_t *ctl, snd_ctl_card_info_t *info) {
    if (fixture_layout && strcmp(fixture_layout, "bad_card_info") == 0) return -EIO;
    (void)ctl; (void)info; return 0;
}
#define MUSIC4 "音楽音楽音楽音楽"
const char *snd_ctl_card_info_get_name(const snd_ctl_card_info_t *info) {
    if (fixture_layout && (strcmp(fixture_layout, "bad_card_name") == 0 ||
                           strcmp(fixture_layout, "missing_card_name") == 0)) return "\xff";
    if (fixture_layout && strcmp(fixture_layout, "long_names") == 0)
        return MUSIC4 MUSIC4 MUSIC4 MUSIC4 MUSIC4 MUSIC4 MUSIC4 MUSIC4;
    if (fixture_layout && strcmp(fixture_layout, "empty_names") == 0) return "";
    (void)info; return "Fixture card 音楽";
}
int snd_card_get_name(int card, char **name) {
    (void)card;
    if (fixture_layout && strcmp(fixture_layout, "missing_card_name") == 0) return -EIO;
    *name = strdup("Fallback card 音楽");
    return *name ? 0 : -ENOMEM;
}
int snd_ctl_pcm_next_device(snd_ctl_t *ctl, int *device) {
    (void)ctl; *device = *device < 3 ? *device + 1 : -1; return 0;
}
int snd_ctl_pcm_info(snd_ctl_t *ctl, snd_pcm_info_t *info) {
    (void)ctl; (void)info; return 0;
}
const char *snd_pcm_info_get_name(const snd_pcm_info_t *info) {
    if (fixture_layout && strcmp(fixture_layout, "bad_name") == 0) return "\xff";
    if (fixture_layout && strcmp(fixture_layout, "long_names") == 0)
        return MUSIC4 MUSIC4 MUSIC4 MUSIC4;
    if (fixture_layout && strcmp(fixture_layout, "empty_names") == 0) return "";
    (void)info; return "Fixture playback";
}
int snd_pcm_open(snd_pcm_t **pcm, const char *name, snd_pcm_stream_t stream, int mode) {
    int (*real_open)(snd_pcm_t **, const char *, snd_pcm_stream_t, int) =
        dlsym(RTLD_NEXT, "snd_pcm_open");
    if (rate_matrix) rate_mask = rate_probe++;
    if (fixture_layout && strcmp(fixture_layout, "probe_error") == 0 && strncmp(name, "hw:", 3) == 0)
        return -ENODEV;
    if (strncmp(name, "hw:", 3) == 0 || strcmp(name, "default") == 0 || strcmp(name, "sysdefault") == 0)
        name = "null";
    return real_open(pcm, name, stream, mode);
}
int snd_pcm_info_get_card(const snd_pcm_info_t *info) {
    (void)info;
    return fixture_layout && strcmp(fixture_layout, "unmatched_default") == 0 ? 2 : 1;
}
unsigned int snd_pcm_info_get_device(const snd_pcm_info_t *info) { (void)info; return 1; }
int snd_pcm_hw_params_test_rate(snd_pcm_t *pcm, snd_pcm_hw_params_t *params, unsigned int rate, int dir) {
    (void)pcm; (void)params; (void)dir;
    if (rate_override) {
        static const unsigned int priority[] = {48000, 44100, 96000, 88200, 192000,
            176400, 32000, 22050, 16000, 11025, 384000};
        for (unsigned int index = 0; index < 11; index++)
            if (rate == priority[index]) return rate_mask & (1U << index) ? 0 : -EINVAL;
        if (rate_edge && strcmp(rate_edge, "reject_edges") == 0) return -EINVAL;
        if (rate_edge && strcmp(rate_edge, "reject_min") == 0 && rate == 8000) return -EINVAL;
        return rate == 8000 || rate == 500000 ? 0 : -EINVAL;
    }
    return 0;
}
int snd_pcm_hw_params_get_rate_min(const snd_pcm_hw_params_t *params, unsigned int *rate, int *dir) {
    (void)params;
    if (rate_edge && strcmp(rate_edge, "min_error") == 0) return -EIO;
    *rate = rate_override ? 8000 : 11025;
    if (rate_edge && strcmp(rate_edge, "min_zero") == 0) *rate = 0;
    if (rate_edge && strcmp(rate_edge, "min_max_reversed") == 0) *rate = 600000;
    if (rate_edge && strcmp(rate_edge, "duplicate_bounds") == 0) *rate = 48000;
    if (dir) *dir = 0;
    return 0;
}
int snd_pcm_hw_params_get_rate_max(const snd_pcm_hw_params_t *params, unsigned int *rate, int *dir) {
    (void)params;
    if (rate_edge && strcmp(rate_edge, "max_error") == 0) return -EIO;
    *rate = rate_override ? 500000 : 384000;
    if (rate_edge && strcmp(rate_edge, "duplicate_bounds") == 0) *rate = 48000;
    if (dir) *dir = 0;
    return 0;
}
