# Compact PipeWire audio widget

The quick-settings popup is now 180 x 240 logical pixels (previously 360 x 490), approximately half the former width and height.

- The header's minimize control hides the popup; clicking the speaker/audio status icon opens it again.
- Output and input master volume sliders and mute toggles remain available.
- Output and input device selectors use left/right arrows to cycle through every device reported by `wpctl`.
- The popup shows two active application streams with individual volume and mute controls. Use **Open full mixer** for additional streams and advanced routing.
- Hiding the popup does not mute audio or change the selected devices.
