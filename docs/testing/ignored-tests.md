# Ignored test inventory

Current source inventory: **258 ignore attributes**. These are source attributes, not macro-expanded test counts. The initial audit had 249.

Six entries in vendored rfd are Linux dialog checks; target platform configuration determines which tests exist in a Windows build.

Only long song-fixture runs are authorized deferrals for this cleanup. The expanded table records explicit passes and permitted long-song deferrals.

| Category | Source attributes |
| --- | ---: |
| benchmark/timing | 191 |
| hardware/display | 22 |
| long song fixture | 38 |
| other | 7 |

| Test or macro | Source | Recorded reason |
| --- | --- | --- |
| `benchmark_texture_worker_batches` | [crates/deadlib-assets/src/decode_perf.rs:66](../../crates/deadlib-assets/src/decode_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_cache_path_allocations` | [crates/deadlib-assets/src/dynamic_perf.rs:149](../../crates/deadlib-assets/src/dynamic_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_cache_file_handle` | [crates/deadlib-assets/src/dynamic_perf.rs:170](../../crates/deadlib-assets/src/dynamic_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `shared_and_exclusive_render_and_release_endpoint` | [crates/deadlib-audio-backend-wasapi/tests/hardware.rs:16](../../crates/deadlib-audio-backend-wasapi/tests/hardware.rs) | requires a Windows audio endpoint with exclusive access enabled |
| `legacy_shared_stream_renders` | [crates/deadlib-audio-backend-wasapi/tests/native/mod.rs:6](../../crates/deadlib-audio-backend-wasapi/tests/native/mod.rs) | requires an available Windows audio endpoint |
| `modern_shared_selects_driver_minimum` | [crates/deadlib-audio-backend-wasapi/tests/native/mod.rs:58](../../crates/deadlib-audio-backend-wasapi/tests/native/mod.rs) | requires a Windows 10/11 endpoint supporting IAudioClient3 |
| `benchmark_runtime_audio_insert` | [crates/deadlib-audio-core/src/position_perf.rs:157](../../crates/deadlib-audio-core/src/position_perf.rs) | paired release benchmark |
| `benchmark_music_gain_ramp` | [crates/deadlib-audio-core/src/render_perf_tests.rs:70](../../crates/deadlib-audio-core/src/render_perf_tests.rs) | paired release benchmark; run alone with --ignored --nocapture --test-threads=1 |
| `benchmark_planar_packet_drain` | [crates/deadlib-audio-decode/src/resample_perf_tests.rs:113](../../crates/deadlib-audio-decode/src/resample_perf_tests.rs) | paired release benchmark; run alone with --ignored --nocapture --test-threads=1 |
| `benchmark_resampler_channel_conversion` | [crates/deadlib-audio-decode/src/resample_perf_tests.rs:155](../../crates/deadlib-audio-decode/src/resample_perf_tests.rs) | paired release benchmark; run alone with --ignored --nocapture --test-threads=1 |
| `benchmark_buffered_wav` | [crates/deadlib-audio-decode/src/wav_performance.rs:161](../../crates/deadlib-audio-decode/src/wav_performance.rs) | paired release throughput and retained-buffer benchmark |
| `benchmark_arc_finalization` | [crates/deadlib-audio/src/stream/perf_tests.rs:195](../../crates/deadlib-audio/src/stream/perf_tests.rs) | paired release benchmark |
| `benchmark_audio_padding_storage` | [crates/deadlib-audio/src/stream/perf_tests.rs:219](../../crates/deadlib-audio/src/stream/perf_tests.rs) | paired release benchmark |
| `benchmark_retained_capture` | [crates/deadlib-present/src/compose_perf.rs:331](../../crates/deadlib-present/src/compose_perf.rs) | paired release throughput benchmark |
| `benchmark_attribute_cursor` | [crates/deadlib-present/src/compose_perf.rs:355](../../crates/deadlib-present/src/compose_perf.rs) | paired release throughput benchmark |
| `benchmark_sprite_clipping` | [crates/deadlib-present/src/compose_perf.rs:395](../../crates/deadlib-present/src/compose_perf.rs) | paired release throughput benchmark |
| `benchmark_core_lookups` | [crates/deadlib-present/src/font_discovery_perf.rs:95](../../crates/deadlib-present/src/font_discovery_perf.rs) | paired release benchmark with warm filesystem cache |
| `mesh_face_modes_select_opposite_winding` | [crates/deadlib-render-backend-gl/src/vertex_state_tests.rs:89](../../crates/deadlib-render-backend-gl/src/vertex_state_tests.rs) | requires OpenGL and a window system |
| `interleaved_vertex_buffers_preserve_pixels` | [crates/deadlib-render-backend-gl/src/vertex_state_tests.rs:161](../../crates/deadlib-render-backend-gl/src/vertex_state_tests.rs) | requires modern OpenGL and a window system |
| `benchmark_environment_geometry` | [crates/deadlib-render-backend-software/src/environment_performance.rs:256](../../crates/deadlib-render-backend-software/src/environment_performance.rs) | paired release renderer throughput benchmark |
| `benchmark_offscreen_alpha_copy` | [crates/deadlib-render-backend-software/src/offscreen_perf_tests.rs:124](../../crates/deadlib-render-backend-software/src/offscreen_perf_tests.rs) | paired release benchmark |
| `mapped_rings_reuse_capacity_and_preserve_frame_offsets` | [crates/deadlib-render-backend-vulkan/src/lib.rs:5373](../../crates/deadlib-render-backend-vulkan/src/lib.rs) | requires a Vulkan graphics device and a window system |
| `texture_upload_recovers` | [crates/deadlib-render-backend-wgpu/src/lib.rs:3757](../../crates/deadlib-render-backend-wgpu/src/lib.rs) | requires a Vulkan device and a window system |
| `surface_resize_preserves_caller_projection` | [crates/deadlib-render-backend-wgpu/src/lib.rs:3801](../../crates/deadlib-render-backend-wgpu/src/lib.rs) | requires OpenGL, Vulkan, and DirectX devices and a window system |
| `textured_backfaces_are_culled` | [crates/deadlib-render-backend-wgpu/src/lib.rs:3892](../../crates/deadlib-render-backend-wgpu/src/lib.rs) | requires a Vulkan device and a window system |
| `pipeline_switches_preserve_camera_pixels` | [crates/deadlib-render-backend-wgpu/src/pipeline_state_tests.rs:198](../../crates/deadlib-render-backend-wgpu/src/pipeline_state_tests.rs) | requires a wgpu graphics device and a window system |
| `ordered_offscreen_pixels` | [crates/deadlib-render-backend-wgpu/tests/offscreen.rs:573](../../crates/deadlib-render-backend-wgpu/tests/offscreen.rs) | requires Windows DX12 and Vulkan adapters with surface readback |
| `benchmark_prepared_sphere_uvs` | [crates/deadlib-render-core/tests/uv_mapper_performance.rs:84](../../crates/deadlib-render-core/tests/uv_mapper_performance.rs) | paired release throughput benchmark |
| `compatible_batches_preserve_backend_pixels` | [crates/deadlib-render/tests/batching.rs:31](../../crates/deadlib-render/tests/batching.rs) | requires graphics devices and a window system; compares pixels within each backend |
| `model_depth_isolation` | [crates/deadlib-render/tests/model_depth.rs:80](../../crates/deadlib-render/tests/model_depth.rs) | requires graphics devices and a window system |
| `gpu_resize_preserves_caller_projection` | [crates/deadlib-render/tests/projection.rs:19](../../crates/deadlib-render/tests/projection.rs) | requires native OpenGL and Vulkan devices and a window system |
| `sphere_material_pixels` | [crates/deadlib-render/tests/sphere_mapping.rs:76](../../crates/deadlib-render/tests/sphere_mapping.rs) | requires graphics devices and a window system |
| `benchmark_replaygain_encoding` | [crates/deadsync-audio-analysis/src/perf_cache_tests.rs:65](../../crates/deadsync-audio-analysis/src/perf_cache_tests.rs) | paired release benchmark |
| `benchmark_metadata_dataflows` | [crates/deadsync-chart/src/metadata_perf.rs:141](../../crates/deadsync-chart/src/metadata_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_resource_traversal` | [crates/deadsync-config/src/resource_paths_perf.rs:99](../../crates/deadsync-config/src/resource_paths_perf.rs) | paired release benchmark |
| `benchmark_state_gameplay` | [crates/deadsync-gameplay/src/gameplay_state_perf.rs:218](../../crates/deadsync-gameplay/src/gameplay_state_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_state_lane_selection` | [crates/deadsync-gameplay/src/gameplay_state_perf.rs:300](../../crates/deadsync-gameplay/src/gameplay_state_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_held_lanes` | [crates/deadsync-gameplay/tests/held_lane_performance.rs:104](../../crates/deadsync-gameplay/tests/held_lane_performance.rs) | paired release throughput benchmark |
| `benchmark_duplicate_import` | [crates/deadsync-import/src/import_perf.rs:309](../../crates/deadsync-import/src/import_perf.rs) | paired release benchmark |
| `benchmark_resolver` | [crates/deadsync-import/src/import_perf.rs:329](../../crates/deadsync-import/src/import_perf.rs) | paired release benchmark |
| `benchmark_settings_import` | [crates/deadsync-import/src/settings_perf.rs:75](../../crates/deadsync-import/src/settings_perf.rs) | paired release benchmark; warm filesystem reads |
| `benchmark_dense_action_storage` | [crates/deadsync-input/src/perf_tests.rs:380](../../crates/deadsync-input/src/perf_tests.rs) | paired release benchmark; run --ignored --nocapture --test-threads=1 |
| `benchmark_static_defaults_loading` | [crates/deadsync-input/src/perf_tests.rs:424](../../crates/deadsync-input/src/perf_tests.rs) | paired release benchmark; run --ignored --nocapture --test-threads=1 |
| `benchmark_owned_binding_edits` | [crates/deadsync-input/src/perf_tests.rs:470](../../crates/deadsync-input/src/perf_tests.rs) | paired release benchmark; run --ignored --nocapture --test-threads=1 |
| `input_pipeline_timing` | [crates/deadsync-input/tests/input_pipeline_timing.rs:26](../../crates/deadsync-input/tests/input_pipeline_timing.rs) | manual release-mode timing; no machine-dependent assertions |
| `benchmark_runtime_lights` | [crates/deadsync-lights/src/cabinet_chart_perf.rs:74](../../crates/deadsync-lights/src/cabinet_chart_perf.rs) | paired release benchmark |
| `benchmark_mesh_capture` | [crates/deadsync-notefield/src/mesh_capture_performance.rs:190](../../crates/deadsync-notefield/src/mesh_capture_performance.rs) | paired release throughput benchmark |
| `benchmark_model_affine` | [crates/deadsync-notefield/src/model_affine_performance.rs:93](../../crates/deadsync-notefield/src/model_affine_performance.rs) | paired release throughput benchmark |
| `benchmark_resource_traversal` | [crates/deadsync-noteskin/src/compiler_resources_perf.rs:71](../../crates/deadsync-noteskin/src/compiler_resources_perf.rs) | paired release benchmark |
| `benchmark_constant_glow_phase` | [crates/deadsync-noteskin/src/explosion_perf_tests.rs:117](../../crates/deadsync-noteskin/src/explosion_perf_tests.rs) | paired release benchmark |
| `benchmark_resource_traversal` | [crates/deadsync-noteskin/src/model_merge_perf.rs:125](../../crates/deadsync-noteskin/src/model_merge_perf.rs) | paired release benchmark |
| `benchmark_ini_parse` | [crates/deadsync-noteskin/src/perf_tests.rs:294](../../crates/deadsync-noteskin/src/perf_tests.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_owned_fallback_merge` | [crates/deadsync-noteskin/src/perf_tests.rs:326](../../crates/deadsync-noteskin/src/perf_tests.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_runtime_cache` | [crates/deadsync-noteskin/src/perf_tests.rs:397](../../crates/deadsync-noteskin/src/perf_tests.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_owned_state_properties` | [crates/deadsync-noteskin/src/sprite_perf_tests.rs:202](../../crates/deadsync-noteskin/src/sprite_perf_tests.rs) | paired release benchmark |
| `benchmark_direct_data_workshop` | [crates/deadsync-noteskin/src/workshop_perf.rs:90](../../crates/deadsync-noteskin/src/workshop_perf.rs) | paired release benchmark; run alone |
| `benchmark_archive_grouping` | [crates/deadsync-online/src/archive_metadata_perf.rs:291](../../crates/deadsync-online/src/archive_metadata_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_borrowed_song_metadata` | [crates/deadsync-online/src/archive_metadata_perf.rs:325](../../crates/deadsync-online/src/archive_metadata_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_borrowed_pagination` | [crates/deadsync-online/src/arrowcloud/pagination_tests.rs:126](../../crates/deadsync-online/src/arrowcloud/pagination_tests.rs) | paired performance benchmark |
| `benchmark_banner_failure_snapshots` | [crates/deadsync-online/src/banners_failure_perf.rs:255](../../crates/deadsync-online/src/banners_failure_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_banner_overflow` | [crates/deadsync-online/src/banners_perf.rs:151](../../crates/deadsync-online/src/banners_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_verdict_serialization` | [crates/deadsync-online/src/beginner_verdict_perf.rs:84](../../crates/deadsync-online/src/beginner_verdict_perf.rs) | paired release benchmark; serialization excludes filesystem latency |
| `benchmark_download_folder_sorting` | [crates/deadsync-online/src/download_sort_perf.rs:161](../../crates/deadsync-online/src/download_sort_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_borrowed_unlock_plan` | [crates/deadsync-online/src/groovestats/unlock_tests.rs:181](../../crates/deadsync-online/src/groovestats/unlock_tests.rs) | paired performance benchmark |
| `benchmark_pack_name_normalization` | [crates/deadsync-online/src/itgdb_names_perf.rs:86](../../crates/deadsync-online/src/itgdb_names_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_song_match_keys` | [crates/deadsync-online/src/match_keys_perf.rs:161](../../crates/deadsync-online/src/match_keys_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_archive_range_merging` | [crates/deadsync-online/src/pack_archive_perf.rs:88](../../crates/deadsync-online/src/pack_archive_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_meter_prefixes_and_beginner_pages` | [crates/deadsync-online/src/pack_meter_perf.rs:84](../../crates/deadsync-online/src/pack_meter_perf.rs) | paired release benchmark |
| `benchmark_pack_subtitles` | [crates/deadsync-online/src/pack_page_perf.rs:61](../../crates/deadsync-online/src/pack_page_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_histogram_numbers` | [crates/deadsync-online/src/pack_response_perf.rs:158](../../crates/deadsync-online/src/pack_response_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_borrowed_pack_rows` | [crates/deadsync-online/src/pack_response_perf.rs:194](../../crates/deadsync-online/src/pack_response_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_completion_borrows` | [crates/deadsync-online/src/player_leaderboards/completion_tests.rs:158](../../crates/deadsync-online/src/player_leaderboards/completion_tests.rs) | paired performance benchmark |
| `benchmark_owned_popularity_results` | [crates/deadsync-online/src/popular_packs_perf.rs:89](../../crates/deadsync-online/src/popular_packs_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_direct_score_callbacks` | [crates/deadsync-online/src/score_import_perf.rs:166](../../crates/deadsync-online/src/score_import_perf.rs) | paired release benchmark |
| `benchmark_catalogue_name_pass` | [crates/deadsync-online/src/smo_catalog_perf.rs:99](../../crates/deadsync-online/src/smo_catalog_perf.rs) | paired release benchmark |
| `benchmark_view_phase_publication` | [crates/deadsync-online/src/smo_describe_perf.rs:266](../../crates/deadsync-online/src/smo_describe_perf.rs) | paired release benchmark |
| `benchmark_view_responses` | [crates/deadsync-online/src/smo_describe_perf.rs:330](../../crates/deadsync-online/src/smo_describe_perf.rs) | paired release benchmark |
| `benchmark_details_pages` | [crates/deadsync-online/src/smo_details_perf.rs:145](../../crates/deadsync-online/src/smo_details_perf.rs) | paired release benchmark |
| `benchmark_preview_and_chart_classification` | [crates/deadsync-online/src/smo_extensions_perf.rs:190](../../crates/deadsync-online/src/smo_extensions_perf.rs) | paired release benchmark |
| `benchmark_repeated_search_requests` | [crates/deadsync-online/src/smo_search_perf.rs:241](../../crates/deadsync-online/src/smo_search_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_search_ordering` | [crates/deadsync-online/src/smo_search_perf.rs:276](../../crates/deadsync-online/src/smo_search_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_percent_encoding` | [crates/deadsync-online/src/smo_search_perf.rs:313](../../crates/deadsync-online/src/smo_search_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_owned_catalog` | [crates/deadsync-online/src/srpg_shop_perf.rs:196](../../crates/deadsync-online/src/srpg_shop_perf.rs) | paired performance benchmark |
| `benchmark_shared_purchase` | [crates/deadsync-online/src/srpg_shop_perf.rs:214](../../crates/deadsync-online/src/srpg_shop_perf.rs) | paired performance benchmark |
| `benchmark_user_context` | [crates/deadsync-online/src/user_context_perf.rs:74](../../crates/deadsync-online/src/user_context_perf.rs) | paired release benchmark |
| `benchmark_profile_sidecar_preparation` | [crates/deadsync-profile/src/perf_tests.rs:364](../../crates/deadsync-profile/src/perf_tests.rs) | paired release benchmark; run alone with --nocapture --test-threads=1 |
| `benchmark_profile_ini_loading` | [crates/deadsync-profile/src/perf_tests.rs:384](../../crates/deadsync-profile/src/perf_tests.rs) | paired release benchmark; run alone with --nocapture --test-threads=1 |
| `benchmark_owned_player_options` | [crates/deadsync-profile/src/perf_tests.rs:413](../../crates/deadsync-profile/src/perf_tests.rs) | paired release benchmark; run alone with --nocapture --test-threads=1 |
| `benchmark_stream_paths` | [crates/deadsync-rules/src/stream_perf.rs:260](../../crates/deadsync-rules/src/stream_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_core_lookups` | [crates/deadsync-rules/src/timing_lookup_perf.rs:348](../../crates/deadsync-rules/src/timing_lookup_perf.rs) | paired release benchmark |
| `benchmark_leaderboard_refresh` | [crates/deadsync-score/src/leaderboard_perf.rs:184](../../crates/deadsync-score/src/leaderboard_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_leaderboard_request_plans` | [crates/deadsync-score/src/leaderboard_request_perf.rs:182](../../crates/deadsync-score/src/leaderboard_request_perf.rs) | paired release benchmark |
| `benchmark_score_snapshots` | [crates/deadsync-score/src/performance_state_flows.rs:148](../../crates/deadsync-score/src/performance_state_flows.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_retry_keys` | [crates/deadsync-score/src/performance_state_flows.rs:286](../../crates/deadsync-score/src/performance_state_flows.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `loose_preview_uploads` | [crates/deadsync-shell/src/app/option_previews.rs:529](../../crates/deadsync-shell/src/app/option_previews.rs) | requires a window system; run alone because asset paths are session-global |
| `sprite_core_gpu_capture_writes_baseline` | [crates/deadsync-shell/src/gameplay_regression_tests.rs:2586](../../crates/deadsync-shell/src/gameplay_regression_tests.rs) | requires an explicitly selected real GPU backend and writes PNG artifacts |
| `lake_hold_bodies_draw_under_lua_player_callback` | [crates/deadsync-shell/src/gameplay_regression_tests.rs:4525](../../crates/deadsync-shell/src/gameplay_regression_tests.rs) | long song-fixture playback and GPU readback regression |
| `song_lua_depth_mask_hides_art_then_opens` | [crates/deadsync-shell/src/gameplay_regression_tests.rs:4854](../../crates/deadsync-shell/src/gameplay_regression_tests.rs) | requires a graphics device |
| `mawaru7_compiled_notefield_transitions` | [crates/deadsync-shell/src/gameplay_regression_tests.rs:4902](../../crates/deadsync-shell/src/gameplay_regression_tests.rs) | long full-song Mawaru transition and GPU readback regression |
| `seventh_saved_mini_reaches_aft_receptors` | [crates/deadsync-shell/src/gameplay_regression_tests.rs:5245](../../crates/deadsync-shell/src/gameplay_regression_tests.rs) | long full-song 7th Gear native geometry regression |
| `bank_account_draws_complete_proxy_stack` | [crates/deadsync-shell/src/gameplay_regression_tests.rs:5871](../../crates/deadsync-shell/src/gameplay_regression_tests.rs) | full-song fixture with GPU readback on Windows |
| `delightful_day_player_proxy_draws_noteskin_instances` | [crates/deadsync-shell/src/gameplay_regression_tests.rs:6069](../../crates/deadsync-shell/src/gameplay_regression_tests.rs) | long full-song compiled render comparison |
| `delightful_day_hides_theme_and_keeps_video` | [crates/deadsync-shell/src/gameplay_regression_tests.rs:6079](../../crates/deadsync-shell/src/gameplay_regression_tests.rs) | long song-fixture video export and GPU readback regression |
| `benchmark_library_artwork` | [crates/deadsync-simfile/src/artwork_library_perf.rs:115](../../crates/deadsync-simfile/src/artwork_library_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_parse_non_media_scan` | [crates/deadsync-simfile/src/bgchanges_parse_perf.rs:82](../../crates/deadsync-simfile/src/bgchanges_parse_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_library_background_handoff` | [crates/deadsync-simfile/src/cache_library_perf.rs:125](../../crates/deadsync-simfile/src/cache_library_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_cache_data_churn` | [crates/deadsync-simfile/src/cache_perf.rs:393](../../crates/deadsync-simfile/src/cache_perf.rs) | paired release benchmark; run serially with --nocapture |
| `benchmark_metadata_courses` | [crates/deadsync-simfile/src/course_ownership_perf.rs:170](../../crates/deadsync-simfile/src/course_ownership_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_metadata_intro` | [crates/deadsync-simfile/src/event_intro_ownership_perf.rs:62](../../crates/deadsync-simfile/src/event_intro_ownership_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_runtime_resource_forks` | [crates/deadsync-simfile/src/media_perf.rs:165](../../crates/deadsync-simfile/src/media_perf.rs) | paired release benchmark |
| `benchmark_metadata_sync` | [crates/deadsync-simfile/src/runtime_cache_ownership_perf.rs:90](../../crates/deadsync-simfile/src/runtime_cache_ownership_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_library_scan` | [crates/deadsync-simfile/src/scan_perf.rs:266](../../crates/deadsync-simfile/src/scan_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_library_chart_bounds` | [crates/deadsync-simfile/src/song_library_perf.rs:70](../../crates/deadsync-simfile/src/song_library_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_selection_search` | [crates/deadsync-simfile/src/song_search_selection_perf.rs:95](../../crates/deadsync-simfile/src/song_search_selection_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_selection_candidate_meters` | [crates/deadsync-simfile/src/song_search_selection_perf.rs:264](../../crates/deadsync-simfile/src/song_search_selection_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_selection_alpha_grouping` | [crates/deadsync-simfile/src/song_sort_selection_perf.rs:100](../../crates/deadsync-simfile/src/song_sort_selection_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_parse_tag_ownership` | [crates/deadsync-simfile/src/tags_parse_perf.rs:85](../../crates/deadsync-simfile/src/tags_parse_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_parse_timing_sort` | [crates/deadsync-simfile/src/timing_parse_perf.rs:192](../../crates/deadsync-simfile/src/timing_parse_perf.rs) | paired release benchmark; run alone with --nocapture |
| `benchmark_streamed_gifs` | [crates/deadsync-smx/src/gifs_perf.rs:156](../../crates/deadsync-smx/src/gifs_perf.rs) | paired performance benchmark |
| `benchmark_actor_state_paths` | [crates/deadsync-song-lua/tests/perf/actor_state.rs:319](../../crates/deadsync-song-lua/tests/perf/actor_state.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_asset_strings` | [crates/deadsync-song-lua/tests/perf/asset_strings.rs:186](../../crates/deadsync-song-lua/tests/perf/asset_strings.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_cmd` | [crates/deadsync-song-lua/tests/perf/cmd.rs:207](../../crates/deadsync-song-lua/tests/perf/cmd.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_cmd_dispatch` | [crates/deadsync-song-lua/tests/perf/cmd_dispatch.rs:234](../../crates/deadsync-song-lua/tests/perf/cmd_dispatch.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_color_inputs` | [crates/deadsync-song-lua/tests/perf/color_inputs.rs:256](../../crates/deadsync-song-lua/tests/perf/color_inputs.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_comparison_sound` | [crates/deadsync-song-lua/tests/perf/comparison_sound.rs:61](../../crates/deadsync-song-lua/tests/perf/comparison_sound.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_comparison_text` | [crates/deadsync-song-lua/tests/perf/comparison_text.rs:125](../../crates/deadsync-song-lua/tests/perf/comparison_text.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_compat_args` | [crates/deadsync-song-lua/tests/perf/compat_args.rs:306](../../crates/deadsync-song-lua/tests/perf/compat_args.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_compile_finalize` | [crates/deadsync-song-lua/tests/perf/compile_finalize.rs:486](../../crates/deadsync-song-lua/tests/perf/compile_finalize.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_draw_capture` | [crates/deadsync-song-lua/tests/perf/draw_capture.rs:487](../../crates/deadsync-song-lua/tests/perf/draw_capture.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_conversion` | [crates/deadsync-song-lua/tests/perf/draw_convert.rs:236](../../crates/deadsync-song-lua/tests/perf/draw_convert.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_file_open` | [crates/deadsync-song-lua/tests/perf/file_open.rs:258](../../crates/deadsync-song-lua/tests/perf/file_open.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_finalization_events` | [crates/deadsync-song-lua/tests/perf/finalization_events.rs:371](../../crates/deadsync-song-lua/tests/perf/finalization_events.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_foreign_assets` | [crates/deadsync-song-lua/tests/perf/foreign_assets.rs:125](../../crates/deadsync-song-lua/tests/perf/foreign_assets.rs) | release performance measurement; run alone with --nocapture |
| `forwarding_trace_keeps_events_arities_errors_and_nested_query_suppression` | [crates/deadsync-song-lua/tests/perf/forward_trace.rs:24](../../crates/deadsync-song-lua/tests/perf/forward_trace.rs) | requires a fresh process for the trace logger; run exact --ignored |
| `benchmark_forward_values` | [crates/deadsync-song-lua/tests/perf/forward_values.rs:179](../../crates/deadsync-song-lua/tests/perf/forward_values.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_ignored_actor` | [crates/deadsync-song-lua/tests/perf/ignored_actor.rs:64](../../crates/deadsync-song-lua/tests/perf/ignored_actor.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_ignored_host` | [crates/deadsync-song-lua/tests/perf/ignored_host.rs:52](../../crates/deadsync-song-lua/tests/perf/ignored_host.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_ignored_tables` | [crates/deadsync-song-lua/tests/perf/ignored_tables.rs:47](../../crates/deadsync-song-lua/tests/perf/ignored_tables.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_material_clock` | [crates/deadsync-song-lua/tests/perf/material_clock.rs:111](../../crates/deadsync-song-lua/tests/perf/material_clock.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_mesh` | [crates/deadsync-song-lua/tests/perf/mesh.rs:327](../../crates/deadsync-song-lua/tests/perf/mesh.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_metatable_storage` | [crates/deadsync-song-lua/tests/perf/metatable_storage.rs:78](../../crates/deadsync-song-lua/tests/perf/metatable_storage.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_method_storage` | [crates/deadsync-song-lua/tests/perf/method_storage.rs:193](../../crates/deadsync-song-lua/tests/perf/method_storage.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_native_binding` | [crates/deadsync-song-lua/tests/perf/native_function.rs:116](../../crates/deadsync-song-lua/tests/perf/native_function.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_option_inputs` | [crates/deadsync-song-lua/tests/perf/option_inputs.rs:396](../../crates/deadsync-song-lua/tests/perf/option_inputs.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_numeric_factories` | [crates/deadsync-song-lua/tests/perf/prefix_args.rs:127](../../crates/deadsync-song-lua/tests/perf/prefix_args.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_query_encode` | [crates/deadsync-song-lua/tests/perf/query_encode.rs:205](../../crates/deadsync-song-lua/tests/perf/query_encode.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_rage_file` | [crates/deadsync-song-lua/tests/perf/rage_file.rs:240](../../crates/deadsync-song-lua/tests/perf/rage_file.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_sound_path` | [crates/deadsync-song-lua/tests/perf/sound_path.rs:148](../../crates/deadsync-song-lua/tests/perf/sound_path.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_snapshot_storage` | [crates/deadsync-song-lua/tests/perf/startup_capture.rs:114](../../crates/deadsync-song-lua/tests/perf/startup_capture.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_text_format` | [crates/deadsync-song-lua/tests/perf/text_format.rs:278](../../crates/deadsync-song-lua/tests/perf/text_format.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_value_dispatch` | [crates/deadsync-song-lua/tests/perf/value_dispatch.rs:331](../../crates/deadsync-song-lua/tests/perf/value_dispatch.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_values` | [crates/deadsync-song-lua/tests/perf/values.rs:216](../../crates/deadsync-song-lua/tests/perf/values.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_method_keys` | [crates/deadsync-song-lua/tests/perf/vm_reads_actor.rs:123](../../crates/deadsync-song-lua/tests/perf/vm_reads_actor.rs) | release performance measurement; run alone with --nocapture |
| `benchmark_registry_and_upvalues` | [crates/deadsync-song-lua/tests/perf/vm_reads_registry.rs:187](../../crates/deadsync-song-lua/tests/perf/vm_reads_registry.rs) | release performance measurement; run alone with --nocapture |
| `manual_player_draws_keep_recorded_pixels` | [crates/deadsync-song-lua/tests/playback/cases.rs:167](../../crates/deadsync-song-lua/tests/playback/cases.rs) | requires a local Vulkan device and window |
| `song_lua_preloaded_aft_chain_preserves_pixels` | [crates/deadsync-song-lua/tests/playback/cases.rs:5711](../../crates/deadsync-song-lua/tests/playback/cases.rs) | requires a Vulkan device and a window system |
| `benchmark_ui_borrowed_state_snapshots` | [crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/leaderboard/borrowed_state_perf.rs:248](../../crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/leaderboard/borrowed_state_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_ui_borrowed_state_render` | [crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/leaderboard/borrowed_state_perf.rs:281](../../crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/leaderboard/borrowed_state_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_ui_borrowed_state_menu` | [crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/menu/borrowed_state_perf.rs:112](../../crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/menu/borrowed_state_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_data_paths_delete_word` | [crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/song_search/data_paths_perf.rs:72](../../crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/song_search/data_paths_perf.rs) | paired release benchmark; run explicitly |
| `benchmark_menu_buffers_completion` | [crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/song_search/menu_buffers_perf.rs:95](../../crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/song_search/menu_buffers_perf.rs) | paired release benchmark; run explicitly |
| `benchmark_transient_buffers_shop_queues` | [crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/srpg_shop_buffers_perf.rs:180](../../crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/srpg_shop_buffers_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_select_data_scorebox` | [crates/deadsync-theme-simply-love/src/screens/components/shared/gs_scorebox/select_data_perf.rs:134](../../crates/deadsync-theme-simply-love/src/screens/components/shared/gs_scorebox/select_data_perf.rs) | paired release benchmark; run serially with --nocapture |
| `benchmark_transient_buffers_profile_counts` | [crates/deadsync-theme-simply-love/src/screens/components/shared/profile_boxes_buffers_perf.rs:31](../../crates/deadsync-theme-simply-love/src/screens/components/shared/profile_boxes_buffers_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `technique_arrow_renders_and_rotates` | [crates/deadsync-theme-simply-love/src/screens/components/shared/technique_bg.rs:485](../../crates/deadsync-theme-simply-love/src/screens/components/shared/technique_bg.rs) | requires a GPU display; run alongside Technique's catalog regression |
| `benchmark_string_pipelines_input` | [crates/deadsync-theme-simply-love/src/screens/components/shared/test_input_pipelines_perf.rs:39](../../crates/deadsync-theme-simply-love/src/screens/components/shared/test_input_pipelines_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_banner_requests` | [crates/deadsync-theme-simply-love/src/screens/content_browser/banner_requests_perf.rs:86](../../crates/deadsync-theme-simply-love/src/screens/content_browser/banner_requests_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_resources_textures` | [crates/deadsync-theme-simply-love/src/screens/content_browser/chart_window/resource_perf.rs:114](../../crates/deadsync-theme-simply-love/src/screens/content_browser/chart_window/resource_perf.rs) | paired release benchmark; run explicitly |
| `benchmark_course_data_chrome` | [crates/deadsync-theme-simply-love/src/screens/content_browser/chrome_data_perf.rs:85](../../crates/deadsync-theme-simply-love/src/screens/content_browser/chrome_data_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_course_data_numbers` | [crates/deadsync-theme-simply-love/src/screens/content_browser/chrome_data_perf.rs:183](../../crates/deadsync-theme-simply-love/src/screens/content_browser/chrome_data_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_detail_fact_table` | [crates/deadsync-theme-simply-love/src/screens/content_browser/detail_facts_perf.rs:105](../../crates/deadsync-theme-simply-love/src/screens/content_browser/detail_facts_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_doubles_rendering` | [crates/deadsync-theme-simply-love/src/screens/content_browser/doubles_render_perf.rs:92](../../crates/deadsync-theme-simply-love/src/screens/content_browser/doubles_render_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_menu_buffers_input` | [crates/deadsync-theme-simply-love/src/screens/content_browser/input/menu_buffers_perf.rs:157](../../crates/deadsync-theme-simply-love/src/screens/content_browser/input/menu_buffers_perf.rs) | paired release benchmark; run explicitly |
| `benchmark_ui_pipelines_preview_selection` | [crates/deadsync-theme-simply-love/src/screens/content_browser/preview_pipelines_perf.rs:117](../../crates/deadsync-theme-simply-love/src/screens/content_browser/preview_pipelines_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_select_data_metadata` | [crates/deadsync-theme-simply-love/src/screens/content_browser/render/select_data_perf.rs:207](../../crates/deadsync-theme-simply-love/src/screens/content_browser/render/select_data_perf.rs) | paired release benchmark; run serially with --nocapture |
| `benchmark_select_data_badges` | [crates/deadsync-theme-simply-love/src/screens/content_browser/render/select_data_perf.rs:246](../../crates/deadsync-theme-simply-love/src/screens/content_browser/render/select_data_perf.rs) | paired release benchmark; run serially with --nocapture |
| `benchmark_select_data_page` | [crates/deadsync-theme-simply-love/src/screens/content_browser/render/select_data_perf.rs:384](../../crates/deadsync-theme-simply-love/src/screens/content_browser/render/select_data_perf.rs) | paired release benchmark; run serially with --nocapture |
| `benchmark_string_pipelines_reload` | [crates/deadsync-theme-simply-love/src/screens/content_browser/render_pipelines_perf.rs:76](../../crates/deadsync-theme-simply-love/src/screens/content_browser/render_pipelines_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_string_pipelines_initials` | [crates/deadsync-theme-simply-love/src/screens/initials_pipelines_perf.rs:152](../../crates/deadsync-theme-simply-love/src/screens/initials_pipelines_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_ui_pipelines_import_summary` | [crates/deadsync-theme-simply-love/src/screens/manage_local_profiles_pipelines_perf.rs:102](../../crates/deadsync-theme-simply-love/src/screens/manage_local_profiles_pipelines_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_view_buffers_replaygain` | [crates/deadsync-theme-simply-love/src/screens/options/apply_replaygain_buffers_perf.rs:84](../../crates/deadsync-theme-simply-love/src/screens/options/apply_replaygain_buffers_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_literal_choices` | [crates/deadsync-theme-simply-love/src/screens/options/choice_text_perf.rs:134](../../crates/deadsync-theme-simply-love/src/screens/options/choice_text_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_view_buffers_pack_catalog` | [crates/deadsync-theme-simply-love/src/screens/options/download_packs_buffers_perf.rs:109](../../crates/deadsync-theme-simply-love/src/screens/options/download_packs_buffers_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_course_data_palette` | [crates/deadsync-theme-simply-love/src/screens/options/palette_data_perf.rs:106](../../crates/deadsync-theme-simply-love/src/screens/options/palette_data_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_view_buffers_reload` | [crates/deadsync-theme-simply-love/src/screens/options/reload_buffers_perf.rs:90](../../crates/deadsync-theme-simply-love/src/screens/options/reload_buffers_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_data_paths_truncation` | [crates/deadsync-theme-simply-love/src/screens/options/render/data_paths_perf.rs:60](../../crates/deadsync-theme-simply-love/src/screens/options/render/data_paths_perf.rs) | paired release benchmark; run explicitly |
| `benchmark_resources_help` | [crates/deadsync-theme-simply-love/src/screens/options/render/resource_perf.rs:107](../../crates/deadsync-theme-simply-love/src/screens/options/render/resource_perf.rs) | paired release benchmark; run explicitly |
| `benchmark_data_paths_pack_refresh` | [crates/deadsync-theme-simply-love/src/screens/options/tests/data_paths_perf.rs:167](../../crates/deadsync-theme-simply-love/src/screens/options/tests/data_paths_perf.rs) | paired release benchmark; run explicitly |
| `benchmark_ui_pipelines_option_navigation` | [crates/deadsync-theme-simply-love/src/screens/options/ui_pipelines_perf.rs:101](../../crates/deadsync-theme-simply-love/src/screens/options/ui_pipelines_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_menu_buffers_packs` | [crates/deadsync-theme-simply-love/src/screens/player_options/pack_options/menu_buffers_perf.rs:157](../../crates/deadsync-theme-simply-love/src/screens/player_options/pack_options/menu_buffers_perf.rs) | paired release benchmark; run explicitly |
| `benchmark_direct_data_rows` | [crates/deadsync-theme-simply-love/src/screens/player_options/panes/direct_data_perf.rs:130](../../crates/deadsync-theme-simply-love/src/screens/player_options/panes/direct_data_perf.rs) | paired release benchmark; run alone |
| `benchmark_transient_buffers_palette_choices` | [crates/deadsync-theme-simply-love/src/screens/player_options/row_buffers_perf.rs:90](../../crates/deadsync-theme-simply-love/src/screens/player_options/row_buffers_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_resources_search` | [crates/deadsync-theme-simply-love/src/screens/player_options/search/resource_perf.rs:247](../../crates/deadsync-theme-simply-love/src/screens/player_options/search/resource_perf.rs) | paired release benchmark; run explicitly |
| `mine_preview_depth_pixels` | [crates/deadsync-theme-simply-love/src/screens/player_options/tests.rs:2376](../../crates/deadsync-theme-simply-love/src/screens/player_options/tests.rs) | requires graphics devices and a window system |
| `benchmark_view_buffers_playlists` | [crates/deadsync-theme-simply-love/src/screens/select_music/playlists_buffers_perf.rs:85](../../crates/deadsync-theme-simply-love/src/screens/select_music/playlists_buffers_perf.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_grade_headings` | [crates/deadsync-theme-simply-love/src/screens/select_music/ui_dataflows_perf.rs:195](../../crates/deadsync-theme-simply-love/src/screens/select_music/ui_dataflows_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_lobby_status` | [crates/deadsync-theme-simply-love/src/screens/select_music/ui_dataflows_perf.rs:378](../../crates/deadsync-theme-simply-love/src/screens/select_music/ui_dataflows_perf.rs) | paired release benchmark; run explicitly with --ignored --nocapture |
| `benchmark_course_loading` | [tests/perf/course_init.rs:170](../../tests/perf/course_init.rs) | paired release benchmark; run explicitly |
| `benchmark_course_loading` | [tests/perf/course_resolve.rs:407](../../tests/perf/course_resolve.rs) | paired release benchmark; run explicitly |
| `benchmark_course_updates` | [tests/perf/course_updates.rs:332](../../tests/perf/course_updates.rs) | paired release benchmark; run explicitly |
| `benchmark_overlay_ownership_ffmpeg` | [tests/perf/ffmpeg_ownership.rs:100](../../tests/perf/ffmpeg_ownership.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_direct_scans` | [tests/perf/graph_range.rs:65](../../tests/perf/graph_range.rs) | paired release benchmark; run explicitly with --nocapture --test-threads=1 |
| `benchmark_runtime_traversal_lobby_order` | [tests/perf/lobby_order.rs:191](../../tests/perf/lobby_order.rs) | paired release benchmark |
| `benchmark_runtime_traversal_lobby_text` | [tests/perf/lobby_text.rs:127](../../tests/perf/lobby_text.rs) | paired release benchmark |
| `benchmark_option_enumeration_sound` | [tests/perf/option_enumeration.rs:286](../../tests/perf/option_enumeration.rs) | paired release benchmark |
| `benchmark_option_enumeration_graphics` | [tests/perf/option_enumeration.rs:364](../../tests/perf/option_enumeration.rs) | paired release benchmark |
| `benchmark_overlay_ownership_updater` | [tests/perf/overlay_ownership.rs:174](../../tests/perf/overlay_ownership.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_owned_pipelines_cache` | [tests/perf/owned_cache.rs:176](../../tests/perf/owned_cache.rs) | paired release benchmark; writes generated cache files to TEMP |
| `benchmark_owned_pipelines_cached` | [tests/perf/owned_cached.rs:87](../../tests/perf/owned_cached.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_owned_pipelines_charts` | [tests/perf/owned_charts.rs:109](../../tests/perf/owned_charts.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `benchmark_runtime_traversal_pack_ini` | [tests/perf/pack_ini.rs:151](../../tests/perf/pack_ini.rs) | paired release benchmark |
| `benchmark_direct_scans` | [tests/perf/parent_anchor.rs:95](../../tests/perf/parent_anchor.rs) | paired release benchmark; run explicitly with --nocapture --test-threads=1 |
| `benchmark_profile_dataflow_option_input` | [tests/perf/player_option_changes.rs:112](../../tests/perf/player_option_changes.rs) | paired release benchmark; run explicitly |
| `benchmark_preview_dataflows_downloads` | [tests/perf/preview_downloads.rs:152](../../tests/perf/preview_downloads.rs) | paired release benchmark; run alone |
| `benchmark_preview_dataflows_models` | [tests/perf/preview_models.rs:91](../../tests/perf/preview_models.rs) | paired release benchmark; run alone |
| `benchmark_preview_dataflows_progress` | [tests/perf/preview_progress.rs:93](../../tests/perf/preview_progress.rs) | paired release benchmark; run alone |
| `benchmark_profile_dataflow_picker` | [tests/perf/profile_picker.rs:157](../../tests/perf/profile_picker.rs) | paired release benchmark; run explicitly |
| `benchmark_profile_dataflow_visual_defaults` | [tests/perf/visual_options.rs:188](../../tests/perf/visual_options.rs) | paired release benchmark; run explicitly |
| `benchmark_direct_scans` | [tests/perf/wheel_meters.rs:134](../../tests/perf/wheel_meters.rs) | paired release benchmark; run explicitly with --nocapture --test-threads=1 |
| `benchmark_overlay_ownership_workshop` | [tests/perf/workshop_ownership.rs:68](../../tests/perf/workshop_ownership.rs) | paired release benchmark; run with --ignored --nocapture --test-threads=1 |
| `lua_song_corpus_smoke_compiles_in_deadsync` | [tests/song_lua_itgmania_parity.rs:291](../../tests/song_lua_itgmania_parity.rs) | long song-corpus compilation, including ten seconds of every archived scene |
| `lua_song_corpus_fully_compiles_in_deadsync` | [tests/song_lua_itgmania_parity.rs:297](../../tests/song_lua_itgmania_parity.rs) | full-song sampling is intentionally an explicit, long-running parity check |
| `reported_song_lua_failures_compile` | [tests/song_lua_itgmania_parity.rs:303](../../tests/song_lua_itgmania_parity.rs) | full-song compile checks for charts without complete reference traces |
| `native_song_lua_semantics_match_deadsync` | [tests/song_lua_itgmania_semantic_parity.rs:6954](../../tests/song_lua_itgmania_semantic_parity.rs) | long full-song semantic replay against the committed Delightful Day capture |
| `cuphead_stateful_fire_message_matches_itgmania` | [tests/song_lua_itgmania_semantic_parity.rs:8280](../../tests/song_lua_itgmania_semantic_parity.rs) | compiles the complete Cuphead song-Lua runtime at 60 Hz |
| `bank_account_complete_semantics_match_itgmania` | [tests/song_lua_itgmania_semantic_parity.rs:8822](../../tests/song_lua_itgmania_semantic_parity.rs) | replays a complete authored song; run explicitly with --ignored |
| `delightful_day_movie_and_hidden_layers_match_itgmania` | [tests/song_lua_itgmania_semantic_parity.rs:8841](../../tests/song_lua_itgmania_semantic_parity.rs) | replays a complete authored song; run explicitly with --ignored |
| `mawaru5_local_draw_colors_match_native` | [tests/song_lua_itgmania_semantic_parity.rs:9972](../../tests/song_lua_itgmania_semantic_parity.rs) | replays a complete authored song; run explicitly with --ignored |
| `mawaru8_local_messages_match_native` | [tests/song_lua_itgmania_semantic_parity.rs:10275](../../tests/song_lua_itgmania_semantic_parity.rs) | replays a complete authored song; run explicitly with --ignored |
| `warp_zone_whole_song_matches_native` | [tests/song_lua_itgmania_semantic_parity.rs:10295](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song semantic and render replay; run explicitly with --ignored |
| `bad_apple_whole_song_matches_native` | [tests/song_lua_itgmania_semantic_parity.rs:10321](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song semantic and render replay; run explicitly with --ignored |
| `goodbye_whole_song_matches_native` | [tests/song_lua_itgmania_semantic_parity.rs:10433](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song semantic and render replay; run explicitly with --ignored |
| `and_drugs_whole_song_matches_native` | [tests/song_lua_itgmania_semantic_parity.rs:10604](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song semantic and render replay; run explicitly with --ignored |
| `igaku_whole_song_matches_native` | [tests/song_lua_itgmania_semantic_parity.rs:10630](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song semantic and render replay; run explicitly with --ignored |
| `mawaru6_whole_song_matches_native` | [tests/song_lua_itgmania_semantic_parity.rs:10667](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song semantic and render replay; run explicitly with --ignored |
| `mawaru7_idle_holds_match_native` | [tests/song_lua_itgmania_semantic_parity.rs:10683](../../tests/song_lua_itgmania_semantic_parity.rs) | replays a complete authored song; run explicitly with --ignored |
| `mawaru7_whole_song_matches_native` | [tests/song_lua_itgmania_semantic_parity.rs:10720](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song semantic and render replay; run explicitly with --ignored |
| `save_tears_whole_native` | [tests/song_lua_itgmania_semantic_parity.rs:10959](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song native geometry and semantic replay; run explicitly with --ignored |
| `i_ai_whole_native` | [tests/song_lua_itgmania_semantic_parity.rs:11040](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song native geometry and semantic replay; run explicitly with --ignored |
| `oshama_whole_native` | [tests/song_lua_itgmania_semantic_parity.rs:11136](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song native geometry and semantic replay; run explicitly with --ignored |
| `sharkmode_whole_song_matches_native` | [tests/song_lua_itgmania_semantic_parity.rs:11210](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song semantic and render replay; run explicitly with --ignored |
| `karachi_whole_native` | [tests/song_lua_itgmania_semantic_parity.rs:11253](../../tests/song_lua_itgmania_semantic_parity.rs) | long whole-song native geometry and semantic replay; run explicitly with --ignored |
| `$name` | [tests/song_lua_itgmania_semantic_parity/corpora.rs:32](../../tests/song_lua_itgmania_semantic_parity/corpora.rs) | compiles one song and reports every semantic check |
| `feelyourtouch_double` | [tests/song_lua_itgmania_semantic_parity/corpora.rs:65](../../tests/song_lua_itgmania_semantic_parity/corpora.rs) | compiles feelyourtouch's double chart and reports every semantic check |
| `song_666_other_difficulties` | [tests/song_lua_itgmania_semantic_parity/corpora.rs:88](../../tests/song_lua_itgmania_semantic_parity/corpora.rs) | compiles every other 666 difficulty and reports all semantic checks |
| `waltz_capriccio_medium` | [tests/song_lua_itgmania_semantic_parity/corpora.rs:101](../../tests/song_lua_itgmania_semantic_parity/corpora.rs) | compiles Waltz's non-Challenge branch and reports every semantic check |
| `ultimate_taste_medium` | [tests/song_lua_itgmania_semantic_parity/corpora.rs:112](../../tests/song_lua_itgmania_semantic_parity/corpora.rs) | compiles Ultimate taste's reduced-mod difficulty and reports every semantic check |
| `edgar_native_hits_match` | [tests/song_lua_itgmania_semantic_parity/multitap.rs:63](../../tests/song_lua_itgmania_semantic_parity/multitap.rs) | replays all 9197 frames of the authored Edgar song; run explicitly with --ignored |
| `edgar_countdown_onsets_and_hit_commands` | [tests/song_lua_itgmania_semantic_parity/multitap.rs:101](../../tests/song_lua_itgmania_semantic_parity/multitap.rs) | replays all 9197 frames of the authored Edgar song; run explicitly with --ignored |
| `native_modifier_values_match_deadsync` | [tests/song_lua_itgmania_semantic_parity/runtime_modifiers.rs:3381](../../tests/song_lua_itgmania_semantic_parity/runtime_modifiers.rs) | long full-song runtime modifier audit against the committed CO5M1C capture |
| `whole_song_archive_index_and_streamed_members_are_valid` | [tests/song_lua_itgmania_semantic_parity/whole_song_archives.rs:901](../../tests/song_lua_itgmania_semantic_parity/whole_song_archives.rs) | long song-fixture audit: streams and validates all 932 committed full-song archives |
| `whole_song_archives_compile_compose_and_match_native_trace` | [tests/song_lua_itgmania_semantic_parity/whole_song_archives.rs:1666](../../tests/song_lua_itgmania_semantic_parity/whole_song_archives.rs) | explicit full-corpus compile, composition, and exact semantic/render audit |
| `message` | [vendor/rfd-win7/src/backend/linux/zenity.rs:249](../../vendor/rfd-win7/src/backend/linux/zenity.rs) | (no reason recorded) |
| `question` | [vendor/rfd-win7/src/backend/linux/zenity.rs:275](../../vendor/rfd-win7/src/backend/linux/zenity.rs) | (no reason recorded) |
| `pick_file` | [vendor/rfd-win7/src/backend/linux/zenity.rs:292](../../vendor/rfd-win7/src/backend/linux/zenity.rs) | (no reason recorded) |
| `pick_files` | [vendor/rfd-win7/src/backend/linux/zenity.rs:299](../../vendor/rfd-win7/src/backend/linux/zenity.rs) | (no reason recorded) |
| `pick_folder` | [vendor/rfd-win7/src/backend/linux/zenity.rs:306](../../vendor/rfd-win7/src/backend/linux/zenity.rs) | (no reason recorded) |
| `save_file` | [vendor/rfd-win7/src/backend/linux/zenity.rs:313](../../vendor/rfd-win7/src/backend/linux/zenity.rs) | (no reason recorded) |

## Expanded names in this build (327)

From the supplied Cargo artifact receipt. CLI harnesses are excluded from libtest enumeration.

| Target | Ignored test | Explicit audit |
| --- | --- | --- |
| `attribute_order` | `compose::performance::benchmark_attribute_cursor` | passed: current artifact |
| `attribute_order` | `compose::performance::benchmark_retained_capture` | passed: current artifact |
| `attribute_order` | `compose::performance::benchmark_sprite_clipping` | passed: current artifact |
| `attribute_order` | `font::discovery_perf::benchmark_core_lookups` | passed: current artifact |
| `batching` | `compatible_batches_preserve_backend_pixels` | passed: current artifact |
| `batching_pixels` | `backend::environment_performance::benchmark_environment_geometry` | passed: identical current benchmark artifact (prior execution) |
| `batching_pixels` | `backend::offscreen_perf_tests::benchmark_offscreen_alpha_copy` | passed: identical current benchmark artifact (prior execution) |
| `deadlib_assets` | `decode::perf_tests::benchmark_texture_worker_batches` | passed: current artifact |
| `deadlib_assets` | `dynamic::perf_tests::benchmark_cache_file_handle` | passed: current artifact |
| `deadlib_assets` | `dynamic::perf_tests::benchmark_cache_path_allocations` | passed: current artifact |
| `deadlib_audio` | `stream::perf_tests::benchmark_arc_finalization` | passed: current artifact |
| `deadlib_audio` | `stream::perf_tests::benchmark_audio_padding_storage` | passed: current artifact |
| `deadlib_audio_backend_wasapi` | `native_tests::legacy_shared_stream_renders` | passed: current artifact |
| `deadlib_audio_backend_wasapi` | `native_tests::modern_shared_selects_driver_minimum` | passed: current artifact |
| `deadlib_audio_core` | `position::perf::benchmark_runtime_audio_insert` | passed: identical current benchmark artifact (prior execution) |
| `deadlib_audio_core` | `render::perf_tests::benchmark_music_gain_ramp` | passed: identical current benchmark artifact (prior execution) |
| `deadlib_audio_decode` | `resample::perf_tests::benchmark_planar_packet_drain` | passed: identical current benchmark artifact (prior execution) |
| `deadlib_audio_decode` | `resample::perf_tests::benchmark_resampler_channel_conversion` | passed: identical current benchmark artifact (prior execution) |
| `deadlib_audio_decode` | `wav::performance::benchmark_buffered_wav` | passed: identical current benchmark artifact (prior execution) |
| `deadlib_present` | `compose::performance::benchmark_attribute_cursor` | passed: current artifact |
| `deadlib_present` | `compose::performance::benchmark_retained_capture` | passed: current artifact |
| `deadlib_present` | `compose::performance::benchmark_sprite_clipping` | passed: current artifact |
| `deadlib_present` | `font::discovery_perf::benchmark_core_lookups` | passed: current artifact |
| `deadlib_render_backend_gl` | `vertex_state_tests::interleaved_vertex_buffers_preserve_pixels` | passed: current artifact |
| `deadlib_render_backend_gl` | `vertex_state_tests::mesh_face_modes_select_opposite_winding` | passed: current artifact |
| `deadlib_render_backend_software` | `environment_performance::benchmark_environment_geometry` | passed: identical current benchmark artifact (prior execution) |
| `deadlib_render_backend_software` | `offscreen_perf_tests::benchmark_offscreen_alpha_copy` | passed: identical current benchmark artifact (prior execution) |
| `deadlib_render_backend_vulkan` | `tests::mapped_rings_reuse_capacity_and_preserve_frame_offsets` | passed: current artifact |
| `deadlib_render_backend_wgpu` | `pipeline_state_tests::pipeline_switches_preserve_camera_pixels` | passed: current artifact |
| `deadlib_render_backend_wgpu` | `tests::surface_resize_preserves_caller_projection` | passed: current artifact |
| `deadlib_render_backend_wgpu` | `tests::texture_upload_recovers` | passed: current artifact |
| `deadlib_render_backend_wgpu` | `tests::textured_backfaces_are_culled` | passed: current artifact |
| `deadsync_audio_analysis` | `cache::perf_tests::benchmark_replaygain_encoding` | passed: current artifact |
| `deadsync_chart` | `song::metadata_perf::benchmark_metadata_dataflows` | passed: current artifact |
| `deadsync_config` | `dirs::resource_perf::benchmark_resource_traversal` | passed: current artifact |
| `deadsync_gameplay` | `state_perf::benchmark_state_gameplay` | passed: current artifact |
| `deadsync_gameplay` | `state_perf::benchmark_state_lane_selection` | passed: current artifact |
| `deadsync_import` | `itg::perf::benchmark_settings_import` | passed: current artifact |
| `deadsync_import` | `pipeline_tests::perf::benchmark_duplicate_import` | passed: current artifact |
| `deadsync_import` | `pipeline_tests::perf::benchmark_resolver` | passed: current artifact |
| `deadsync_input` | `keymap::perf_tests::benchmark_dense_action_storage` | passed: current artifact |
| `deadsync_input` | `keymap::perf_tests::benchmark_owned_binding_edits` | passed: current artifact |
| `deadsync_input` | `keymap::perf_tests::benchmark_static_defaults_loading` | passed: current artifact |
| `deadsync_lights` | `cabinet_chart::tests::perf::benchmark_runtime_lights` | passed: current artifact |
| `deadsync_notefield` | `field_frame::capture_performance::benchmark_mesh_capture` | passed: current artifact |
| `deadsync_notefield` | `noteskin_model::affine_performance::benchmark_model_affine` | passed: current artifact |
| `deadsync_noteskin` | `compiler::resource_perf::benchmark_resource_traversal` | passed: current artifact |
| `deadsync_noteskin` | `explosion::explosion_perf_tests::benchmark_constant_glow_phase` | passed: current artifact |
| `deadsync_noteskin` | `itg::perf_tests::benchmark_ini_parse` | passed: current artifact |
| `deadsync_noteskin` | `itg::perf_tests::benchmark_owned_fallback_merge` | passed: current artifact |
| `deadsync_noteskin` | `itg::perf_tests::benchmark_runtime_cache` | passed: current artifact |
| `deadsync_noteskin` | `model::resource_perf::benchmark_resource_traversal` | passed: current artifact |
| `deadsync_noteskin` | `sprite::sprite_perf_tests::benchmark_owned_state_properties` | passed: current artifact |
| `deadsync_noteskin` | `workshop::direct_data_perf::benchmark_direct_data_workshop` | passed: current artifact |
| `deadsync_online` | `arrowcloud::pagination_tests::benchmark_borrowed_pagination` | passed: current artifact |
| `deadsync_online` | `arrowcloud::user_context_perf::benchmark_user_context` | passed: current artifact |
| `deadsync_online` | `banners::banners_failure_perf_tests::benchmark_banner_failure_snapshots` | passed: current artifact |
| `deadsync_online` | `banners::perf_tests::benchmark_banner_overflow` | passed: current artifact |
| `deadsync_online` | `beginner::verdict_perf_tests::benchmark_verdict_serialization` | passed: current artifact |
| `deadsync_online` | `downloads::download_sort_perf_tests::benchmark_download_folder_sorting` | passed: current artifact |
| `deadsync_online` | `groovestats::tests::benchmark_borrowed_unlock_plan` | passed: current artifact |
| `deadsync_online` | `itgdb::names_perf_tests::benchmark_pack_name_normalization` | passed: current artifact |
| `deadsync_online` | `pack_archive::match_keys_perf_tests::benchmark_song_match_keys` | passed: current artifact |
| `deadsync_online` | `pack_archive::metadata_perf_tests::benchmark_archive_grouping` | passed: current artifact |
| `deadsync_online` | `pack_archive::metadata_perf_tests::benchmark_borrowed_song_metadata` | passed: current artifact |
| `deadsync_online` | `pack_archive::perf_tests::benchmark_archive_range_merging` | passed: current artifact |
| `deadsync_online` | `pack_page::meter_perf_tests::benchmark_meter_prefixes_and_beginner_pages` | passed: current artifact |
| `deadsync_online` | `pack_page::perf_tests::benchmark_pack_subtitles` | passed: current artifact |
| `deadsync_online` | `pack_page::response_perf_tests::benchmark_borrowed_pack_rows` | passed: current artifact |
| `deadsync_online` | `pack_page::response_perf_tests::benchmark_histogram_numbers` | passed: current artifact |
| `deadsync_online` | `player_leaderboards::completion_tests::benchmark_completion_borrows` | passed: current artifact |
| `deadsync_online` | `popular_packs::perf_tests::benchmark_owned_popularity_results` | passed: current artifact |
| `deadsync_online` | `score_import::perf_tests::benchmark_direct_score_callbacks` | passed: current artifact |
| `deadsync_online` | `smo_describe::perf_tests::benchmark_view_phase_publication` | passed: current artifact |
| `deadsync_online` | `smo_describe::perf_tests::benchmark_view_responses` | passed: current artifact |
| `deadsync_online` | `smo_details::perf_tests::benchmark_details_pages` | passed: current artifact |
| `deadsync_online` | `smo_search::catalog_perf_tests::benchmark_catalogue_name_pass` | passed: current artifact |
| `deadsync_online` | `smo_search::perf_tests::benchmark_percent_encoding` | passed: current artifact |
| `deadsync_online` | `smo_search::perf_tests::benchmark_repeated_search_requests` | passed: current artifact |
| `deadsync_online` | `smo_search::perf_tests::benchmark_search_ordering` | passed: current artifact |
| `deadsync_online` | `smo_songs::extension_perf_tests::benchmark_preview_and_chart_classification` | passed: current artifact |
| `deadsync_online` | `srpg_shop::perf_tests::benchmark_owned_catalog` | passed: current artifact |
| `deadsync_online` | `srpg_shop::perf_tests::benchmark_shared_purchase` | passed: current artifact |
| `deadsync_profile` | `perf_tests::benchmark_owned_player_options` | passed: current artifact |
| `deadsync_profile` | `perf_tests::benchmark_profile_ini_loading` | passed: current artifact |
| `deadsync_profile` | `perf_tests::benchmark_profile_sidecar_preparation` | passed: current artifact |
| `deadsync_profile` | `visual_options_perf::benchmark_profile_dataflow_visual_defaults` | passed: current artifact |
| `deadsync_rules` | `stream::stream_perf::benchmark_stream_paths` | passed: current artifact |
| `deadsync_rules` | `timing::lookup_perf::benchmark_core_lookups` | passed: current artifact |
| `deadsync_score` | `leaderboard::performance_refresh::benchmark_leaderboard_refresh` | passed: current artifact |
| `deadsync_score` | `leaderboard::request_perf::benchmark_leaderboard_request_plans` | passed: current artifact |
| `deadsync_score` | `performance_state_flows::benchmark_retry_keys` | passed: current artifact |
| `deadsync_score` | `performance_state_flows::benchmark_score_snapshots` | passed: current artifact |
| `deadsync_shell` | `app::option_previews::tests::loose_preview_uploads` | passed: current artifact |
| `deadsync_shell` | `content_reload::perf_traversal::benchmark_runtime_traversal_pack_ini` | passed: current artifact |
| `deadsync_shell` | `course::update_tests::benchmark_course_updates` | passed: current artifact |
| `deadsync_shell` | `gameplay_entry::owned_pipeline_tests::benchmark_owned_pipelines_charts` | passed: current artifact |
| `deadsync_shell` | `gameplay_regression_tests::tests::bank_account_draws_complete_proxy_stack` | deferred: long song fixture |
| `deadsync_shell` | `gameplay_regression_tests::tests::delightful_day_hides_theme_and_keeps_video` | deferred: long song fixture |
| `deadsync_shell` | `gameplay_regression_tests::tests::delightful_day_player_proxy_draws_noteskin_instances` | deferred: long song fixture |
| `deadsync_shell` | `gameplay_regression_tests::tests::lake_hold_bodies_draw_under_lua_player_callback` | deferred: long song fixture |
| `deadsync_shell` | `gameplay_regression_tests::tests::mawaru7_compiled_notefield_transitions` | deferred: long song fixture |
| `deadsync_shell` | `gameplay_regression_tests::tests::seventh_saved_mini_reaches_aft_receptors` | deferred: long song fixture |
| `deadsync_shell` | `gameplay_regression_tests::tests::song_lua_depth_mask_hides_art_then_opens` | passed: current artifact |
| `deadsync_shell` | `gameplay_regression_tests::tests::sprite_core_gpu_capture_writes_baseline` | passed: current artifact (opengl, vulkan, vulkan-wgpu) |
| `deadsync_shell` | `local_profiles::dataflow_tests::benchmark_profile_dataflow_picker` | passed: current artifact |
| `deadsync_shell` | `sync_analysis::owned_pipeline_tests::benchmark_owned_pipelines_cached` | passed: current artifact |
| `deadsync_shell` | `sync_analysis_cache::owned_pipeline_tests::benchmark_owned_pipelines_cache` | passed: current artifact |
| `deadsync_simfile` | `artwork::tests::library_perf::benchmark_library_artwork` | passed: current artifact |
| `deadsync_simfile` | `bgchanges::parse_perf::benchmark_parse_non_media_scan` | passed: current artifact |
| `deadsync_simfile` | `cache::tests::data_churn::benchmark_cache_data_churn` | passed: current artifact |
| `deadsync_simfile` | `cache::tests::library_perf::benchmark_library_background_handoff` | passed: current artifact |
| `deadsync_simfile` | `course::tests::loading_perf::benchmark_course_loading` | passed: current artifact |
| `deadsync_simfile` | `course::tests::ownership_perf::benchmark_metadata_courses` | passed: current artifact |
| `deadsync_simfile` | `event_intro::tests::ownership_perf::benchmark_metadata_intro` | passed: current artifact |
| `deadsync_simfile` | `media::perf::benchmark_runtime_resource_forks` | passed: current artifact |
| `deadsync_simfile` | `runtime_cache::tests::ownership_perf::benchmark_metadata_sync` | passed: current artifact |
| `deadsync_simfile` | `scan::tests::perf::benchmark_library_scan` | passed: current artifact |
| `deadsync_simfile` | `song::tests::library_perf::benchmark_library_chart_bounds` | passed: current artifact |
| `deadsync_simfile` | `song_search::tests::benchmark_selection_candidate_meters` | passed: current artifact |
| `deadsync_simfile` | `song_search::tests::benchmark_selection_search` | passed: current artifact |
| `deadsync_simfile` | `song_sort::tests::benchmark_selection_alpha_grouping` | passed: current artifact |
| `deadsync_simfile` | `tags::parse_perf::benchmark_parse_tag_ownership` | passed: current artifact |
| `deadsync_simfile` | `timing::parse_perf::benchmark_parse_timing_sort` | passed: current artifact |
| `deadsync_smx` | `gifs::perf_tests::benchmark_streamed_gifs` | passed: current artifact |
| `deadsync_song_lua` | `actor_classes::forward_values_perf::benchmark_forward_values` | passed: current artifact |
| `deadsync_song_lua` | `actor_classes::method_storage_perf::benchmark_method_storage` | passed: current artifact |
| `deadsync_song_lua` | `actor_classes::native_function_perf::benchmark_native_binding` | passed: current artifact |
| `deadsync_song_lua` | `actor_classes::vm_reads_perf::benchmark_method_keys` | passed: current artifact |
| `deadsync_song_lua` | `cmd::dispatch_perf::benchmark_cmd_dispatch` | passed: current artifact |
| `deadsync_song_lua` | `cmd::perf::benchmark_cmd` | passed: current artifact |
| `deadsync_song_lua` | `compile::finalization_events_perf::benchmark_finalization_events` | passed: current artifact |
| `deadsync_song_lua` | `compile::perf::benchmark_compile_finalize` | passed: current artifact |
| `deadsync_song_lua` | `draw_capture::perf::benchmark_draw_capture` | passed: current artifact |
| `deadsync_song_lua` | `draw_capture::perf::conversion::benchmark_conversion` | passed: current artifact |
| `deadsync_song_lua` | `host::comparison_sound_perf::benchmark_comparison_sound` | passed: current artifact |
| `deadsync_song_lua` | `host::ignored_args_perf::benchmark_ignored_host` | passed: current artifact |
| `deadsync_song_lua` | `host::sound_path_perf::benchmark_sound_path` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::actor_state_perf::benchmark_actor_state_paths` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::asset_strings_perf::benchmark_asset_strings` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::comparison_text_perf::benchmark_comparison_text` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::foreign_assets_perf::benchmark_foreign_assets` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::ignored_args_perf::benchmark_ignored_actor` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::mesh_perf::benchmark_mesh` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::metatable_storage_perf::benchmark_metatable_storage` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::prefix_args_perf::benchmark_numeric_factories` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::startup_capture_perf::benchmark_snapshot_storage` | passed: current artifact |
| `deadsync_song_lua` | `lua_util::vm_reads_perf::benchmark_registry_and_upvalues` | passed: current artifact |
| `deadsync_song_lua` | `model_texture::perf::benchmark_material_clock` | passed: current artifact |
| `deadsync_song_lua` | `net::perf::benchmark_query_encode` | passed: current artifact |
| `deadsync_song_lua` | `option_rows::input_perf::benchmark_option_inputs` | passed: current artifact |
| `deadsync_song_lua` | `rage_file::file_open_perf::benchmark_file_open` | passed: current artifact |
| `deadsync_song_lua` | `rage_file::perf::benchmark_rage_file` | passed: current artifact |
| `deadsync_song_lua` | `string_args::perf::benchmark_compat_args` | passed: current artifact |
| `deadsync_song_lua` | `tables::ignored_args_perf::benchmark_ignored_tables` | passed: current artifact |
| `deadsync_song_lua` | `tables::text_perf::benchmark_text_format` | passed: current artifact |
| `deadsync_song_lua` | `theme_colors::input_perf::benchmark_color_inputs` | passed: current artifact |
| `deadsync_song_lua` | `values::dispatch_perf::benchmark_value_dispatch` | passed: current artifact |
| `deadsync_song_lua` | `values::perf::benchmark_values` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::evaluation::event_progress::preview_dataflows::benchmark_preview_dataflows_progress` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::lobby_overlay::perf_traversal::benchmark_runtime_traversal_lobby_text` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::music_wheel::tests::direct_scan_perf::benchmark_direct_scans` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::select_music_menu::downloads::preview_dataflows::benchmark_preview_dataflows_downloads` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::select_music_menu::leaderboard::borrowed_state_perf::benchmark_ui_borrowed_state_render` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::select_music_menu::leaderboard::borrowed_state_perf::benchmark_ui_borrowed_state_snapshots` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::select_music_menu::menu::borrowed_state_perf::benchmark_ui_borrowed_state_menu` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::select_music_menu::song_search::data_paths_perf::benchmark_data_paths_delete_word` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::select_music_menu::song_search::menu_buffers_perf::benchmark_menu_buffers_completion` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::select_music::select_music_menu::srpg_shop::buffers_perf::benchmark_transient_buffers_shop_queues` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::shared::ffmpeg_overlay::perf_tests::benchmark_overlay_ownership_ffmpeg` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::shared::gs_scorebox::select_data_perf::benchmark_select_data_scorebox` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::shared::lobby_hud::perf_traversal::benchmark_runtime_traversal_lobby_order` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::shared::profile_boxes::buffers_perf::benchmark_transient_buffers_profile_counts` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::shared::technique_bg::tests::technique_arrow_renders_and_rotates` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::shared::test_input::pipelines_perf::benchmark_string_pipelines_input` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::components::shared::update_overlay::perf_tests::benchmark_overlay_ownership_updater` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::chart_window::preview_dataflows::benchmark_preview_dataflows_models` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::chart_window::resource_perf::benchmark_resources_textures` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::detail::detail_facts_perf_tests::benchmark_detail_fact_table` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::input::menu_buffers_perf::benchmark_menu_buffers_input` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::preview::pipelines_perf::benchmark_ui_pipelines_preview_selection` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::render::course_data_perf::benchmark_course_data_chrome` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::render::course_data_perf::benchmark_course_data_numbers` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::render::doubles_render_perf_tests::benchmark_doubles_rendering` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::render::pipelines_perf::benchmark_string_pipelines_reload` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::render::select_data_perf::benchmark_select_data_badges` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::render::select_data_perf::benchmark_select_data_metadata` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::render::select_data_perf::benchmark_select_data_page` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::content_browser::state::banner_requests_perf_tests::benchmark_banner_requests` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::initials::pipelines_perf::benchmark_string_pipelines_initials` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::manage_local_profiles::pipelines_perf::benchmark_ui_pipelines_import_summary` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::apply_replaygain::buffers_perf::benchmark_view_buffers_replaygain` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::choice_text::choice_text_perf_tests::benchmark_literal_choices` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::download_packs::buffers_perf::benchmark_view_buffers_pack_catalog` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::downloads::perf_tests::benchmark_overlay_ownership_workshop` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::judgment_palettes::course_data_perf::benchmark_course_data_palette` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::reload::buffers_perf::benchmark_view_buffers_reload` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::render::data_paths_perf::benchmark_data_paths_truncation` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::render::resource_perf::benchmark_resources_help` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::tests::data_paths_perf::benchmark_data_paths_pack_refresh` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::tests::enumeration::benchmark_option_enumeration_graphics` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::tests::enumeration::benchmark_option_enumeration_sound` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::options::tests::pipelines_perf::benchmark_ui_pipelines_option_navigation` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::player_options::pack_options::menu_buffers_perf::benchmark_menu_buffers_packs` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::player_options::panes::direct_data_perf::benchmark_direct_data_rows` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::player_options::row::buffers_perf::benchmark_transient_buffers_palette_choices` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::player_options::search::resource_perf::benchmark_resources_search` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::player_options::tests::tests::benchmark_profile_dataflow_option_input` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::player_options::tests::tests::mine_preview_depth_pixels` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::player_options::visibility::direct_scan_perf::benchmark_direct_scans` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::select_course::song_lookup_tests::loading_perf::benchmark_course_loading` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::select_music::playlists::buffers_perf::benchmark_view_buffers_playlists` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::select_music::sync_graph::direct_scan_perf::benchmark_direct_scans` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::select_music::ui_dataflows_perf_tests::benchmark_grade_headings` | passed: current artifact |
| `deadsync_theme_simply_love` | `screens::select_music::ui_dataflows_perf_tests::benchmark_lobby_status` | passed: current artifact |
| `hardware` | `shared_and_exclusive_render_and_release_endpoint` | passed: current artifact |
| `held_lane_performance` | `benchmark_held_lanes` | passed: current artifact |
| `input_pipeline_timing` | `input_pipeline_timing` | passed: current artifact |
| `masked_render` | `compose::performance::benchmark_attribute_cursor` | passed: current artifact |
| `masked_render` | `compose::performance::benchmark_retained_capture` | passed: current artifact |
| `masked_render` | `compose::performance::benchmark_sprite_clipping` | passed: current artifact |
| `masked_render` | `font::discovery_perf::benchmark_core_lookups` | passed: current artifact |
| `model_depth` | `model_depth_isolation` | passed: current artifact |
| `offscreen` | `ordered_offscreen_pixels` | passed: current artifact |
| `playback` | `playback::tests::manual_player_draws_keep_recorded_pixels` | passed: current artifact |
| `playback` | `playback::tests::song_lua_preloaded_aft_chain_preserves_pixels` | passed: current artifact |
| `projection` | `gpu_resize_preserves_caller_projection` | passed: current artifact |
| `song_lua_itgmania_parity` | `lua_song_corpus_fully_compiles_in_deadsync` | deferred: long song fixture |
| `song_lua_itgmania_parity` | `lua_song_corpus_smoke_compiles_in_deadsync` | deferred: long song fixture |
| `song_lua_itgmania_parity` | `reported_song_lua_failures_compile` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `and_drugs_whole_song_matches_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `bad_apple_whole_song_matches_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `bank_account_complete_semantics_match_itgmania` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::allowed::cosmic_railroad` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::allowed::flip69` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::allowed::gemini_in_clockland` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::allowed::godspeed` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::allowed::kenpo_saito` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::allowed::media_offline` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::allowed::riddle` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::allowed::spooky` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::and_drugs` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::apollo` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::bad_apple` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::bank_account` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::botanic_panic` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::brain_power` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::broadcast` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::brogamer` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::bunny_house` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::chikutaku` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::circles` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::crystal_access` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::cursed_metamorph` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::delightful_day` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::do_the_mario` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::feelyourtouch` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::feelyourtouch_double` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::finite_edit` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::flowers` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::flying_castle` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::fof_guys` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::goodbye` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::i_ai` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::igaku` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::jumper` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::kagetsu_no_yume` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::karachi` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::lake_of_lost_nostalgia` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::lala` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::let_me_hear_that` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::levels` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::macarena` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::mawaru2` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::mawaru3` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::mawaru4` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::mawaru5` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::mawaru6` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::mawaru7` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::mawaru8` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::mawaru9` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::megalovania` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::my_exes_bestfriend` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::oshama_scramble` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::pale` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::panopticon` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::rainbow_road` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::save_your_tears` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::seventh_gear` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::sharkmode` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::slamurai` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::someone_special` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::song_666` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::song_666_other_difficulties` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::spectrum_sequence` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::step_your_game_up` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::ultimate_taste` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::ultimate_taste_medium` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::waltz_capriccio` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::waltz_capriccio_medium` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::warp_zone` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `corpora::lua_songs::who_the_hell_is_edgar` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `cuphead_stateful_fire_message_matches_itgmania` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `delightful_day_movie_and_hidden_layers_match_itgmania` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `goodbye_whole_song_matches_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `i_ai_whole_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `igaku_whole_song_matches_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `karachi_whole_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `mawaru5_local_draw_colors_match_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `mawaru6_whole_song_matches_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `mawaru7_idle_holds_match_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `mawaru7_whole_song_matches_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `mawaru8_local_messages_match_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `multitap::edgar_countdown_onsets_and_hit_commands` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `multitap::edgar_native_hits_match` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `native_song_lua_semantics_match_deadsync` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `oshama_whole_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `runtime_modifiers::native_modifier_values_match_deadsync` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `save_tears_whole_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `sharkmode_whole_song_matches_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `warp_zone_whole_song_matches_native` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `whole_song_archives::whole_song_archive_index_and_streamed_members_are_valid` | deferred: long song fixture |
| `song_lua_itgmania_semantic_parity` | `whole_song_archives::whole_song_archives_compile_compose_and_match_native_trace` | deferred: long song fixture |
| `sphere_mapping` | `sphere_material_pixels` | passed: current artifact |
| `stripe_bins` | `backend::environment_performance::benchmark_environment_geometry` | passed: identical current benchmark artifact (prior execution) |
| `stripe_bins` | `backend::offscreen_perf_tests::benchmark_offscreen_alpha_copy` | passed: identical current benchmark artifact (prior execution) |
| `uv_mapper_performance` | `benchmark_prepared_sphere_uvs` | passed: current artifact |
