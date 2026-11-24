# Building DeadSync.app

Run these commands on macOS from the repository root:

```sh
xcode-select --install
brew install cmake vulkan-loader molten-vk
python3 scripts/build-macos-app.py --profile local
open dist/DeadSync.app
```

Rust stable and Python 3 are required. `local` is an optimized development
profile with faster linking. Omit `--profile local` for the fully optimized
release build. For subsequent builds, pass `--replace` to replace the generated
application after the new bundle is signed successfully. Only an app with the same
bundle identifier can be replaced. Alternatively choose
`--output /absolute/path/DeadSync.app`.

The application contains:

```text
DeadSync.app/Contents/
  Info.plist
  MacOS/deadsync
  Frameworks/                 Vulkan loader, MoltenVK, native dependencies
  Resources/
    DeadSync.icns
    assets/
    songs/                   Only repository-tracked content
    courses/
    vulkan/icd.d/MoltenVK_icd.json
    LICENSE
```

The packager recursively copies non-system dylibs, rewrites their install names,
checks their CPU architectures, and signs the resulting application. Homebrew is
needed on the build machine, but the application ships its Vulkan runtime.
The deployment version in Info.plist is derived from the executable and libraries;
bundling a library built for a newer macOS also raises the application's minimum.
Apple Silicon and Intel applications must each be built with compatible libraries.

Configuration, profiles, added songs and scores remain in
`~/Library/Application Support/deadsync`; caches remain in
`~/Library/Caches/deadsync`. Do not put user data or `portable.txt` inside the
application. Keyboard navigation and play work without Input Monitoring: when
raw IOHID keyboard access is denied or unavailable, the game uses keyboard events
from its focused window. To use hardware-timestamped keyboard input, optionally
enable Input Monitoring for **DeadSync** in System Settings → Privacy & Security
and restart the application. Raw keyboard events are ignored while the window is
inactive, including in Debug logs.

## Package an existing build

```sh
python3 scripts/build-macos-app.py --binary target/release/deadsync --zip
```

This creates the application, a ZIP archive and its SHA-256 checksum. It packages
the working checkout's resources, so use an executable built from that checkout.
`MOLTENVK_LIBRARY` can select a non-Homebrew `libMoltenVK.dylib`.

The default signature is ad-hoc, suitable for local development. For distribution,
pass `--sign "Developer ID Application: Your Name (TEAMID)"`, then notarize and
staple the application using your Apple Developer credentials before distributing
it. This script does not submit anything to Apple or publish a release.

The existing loose-file tar updater is disabled inside an application bundle:
installing that format would break the signed application. Replace the entire
`.app` to upgrade it. Existing tar releases retain their current updater behavior.

Vulkan layout reference:
[LunarG's application bundle guide](https://vulkan.lunarg.com/doc/view/latest/mac/getting_started.html).
