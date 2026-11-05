Runtime assets for the browser build.

The music and text fonts (Bravura, Noto Sans) are compiled into guitarscore_bg.wasm, so
the editor works without fetching them; their licences are in assets/fonts/ in the source
tree and in THIRD_PARTY.md.

`cargo xtask web` downloads GeneralUser GS 2.0.3 (pinned and checked) into
dist/web/assets/soundfonts/ unless `--soundfont PATH` or `--no-soundfont` is given; the
license and the manifest the page reads are written next to it. The .sf2 is not committed.

icon.png is the app icon for the manifest and the page favicon.
