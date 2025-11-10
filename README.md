# DeadSync

DeadSync, as in "dead on sync", is a StepMania/ITG engine with Vulkan/OpenGL backends, focused on perfect sync and competitive-level performance, written entirely in Rust. Cross-platform: Windows / Linux / BSD / macOS – x86_64 & ARM64.

⚠️ **DeadSync is under heavy development. Bugs are expected, features may change, and things *will* break. You have been warned.**

## Prerequisites

Before building, ensure you have the following installed on your system:

1.  **Rust**: Install via [rustup](https://rustup.rs/).
2.  **Vulkan SDK**: Download and install the SDK for your operating system from the [LunarG website](https://www.lunarg.com/vulkan-sdk/).

### Linux build dependencies (Ubuntu/Debian)
```bash
sudo apt update
sudo apt install --no-install-recommends build-essential cmake pkg-config libudev-dev libasound2-dev libvulkan-dev libgl1-mesa-dev
```

## Getting Started

Follow these steps to get the game running:

1.  **Clone the Repository:**
    ```sh
    git clone --recurse-submodules https://github.com/pnn64/deadsync.git
    cd deadsync
    ```

2.  **Add Songs:**
    Create a folder named `songs` in the project root. Place your song packs inside this directory.
    *   *Example structure: `deadsync/songs/MyPack/MySong/MySong.ssc`*

3.  **Build the Project:**
    Compile the game in release mode for optimal performance:
    ```sh
    cargo build --release
    ```

4.  **Run the Game:**
    After a successful build, run the executable from the project root:

    *   **On Windows:**
        ```sh
        .\target\release\deadsync.exe
        ```
    *   **On Linux or macOS:**
        ```sh
        ./target/release/deadsync
        ```

## Configuration

After running the game for the first time, configuration files and a `save` directory will be generated in the project root.

### Game Settings
You can edit `deadsync.ini` to change various settings, including renderer, video resolution, VSync, and the default theme color.

### Profile & Online Features
A `save` directory is also created to store your personal data.

*   To enable online features with **GrooveStats**, edit the `save/profiles/00000000/groovestats.ini` file and add your API key and username. This allows the game to fetch your online scores.
*   You can also change your in-game display name in `save/profiles/00000000/profile.ini`.
