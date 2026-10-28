# NIRUNOTE 0.5.0

Responsive NIRUX writing release built from 0.4.6.

- Follow NIRUX only where NIRUX is detected; live theme changes preserve the document, selection, cursor and scroll state. Debian/Omarchy keep their existing dark/light/Satie setting.
- Central palette resolution with a safe dark fallback for unknown theme names.
- Spell probing, Quick Open scanning, recovery writes and large-document Preview rendering move off the GUI thread; Preview is debounced and cached.
- Preview links are restricted to http, https and mailto; attributes are escaped and anchor clicks are checked again before opening.
- Status shows saved/modified state, NIRUX theme, word/character counts, language and recovery status. Quick Open can be cancelled and key dialogs expose accessible names.
- Extract the Markdown renderer and Hunspell dictionary into modules without changing the CLI, installer layout or user data model.

## Validation limitations

Live Wayland/Hyprland preview scrolling, a real NIRUX theme switch, external file changes and recovery-after-crash require on-device acceptance. Hunspell runtime validation depends on the installed Swedish dictionary.
