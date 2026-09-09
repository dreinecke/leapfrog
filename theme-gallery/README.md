# Leapfrog Omarchy Theme Gallery

Open `index.html` in a browser to browse the Omarchy themes installed on this
machine. The page includes theme thumbnails, all available backgrounds, and
every hex color mapping found in each effective `colors.toml`.

Regenerate the gallery after installing or editing themes:

```sh
./generate_gallery.py
```

The generator reads stock themes, user themes, and user-added backgrounds. It
does not modify Omarchy's packaged or user configuration.
