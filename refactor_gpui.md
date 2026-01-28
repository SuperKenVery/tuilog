Rewrite this project's GUI from dioxus to GPUI.

Note that in order to build with GPUI, a nix flake environment would be problematic because it lacks the metal tool.
You'll need to `set -e SDKROOT DEVELOPER_DIR` and then `fish_add_path /usr/bin` in order to use the `metal` command (which allows cargo build to access it too)
