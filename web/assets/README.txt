Runtime assets for the browser build.

The music and text fonts (Bravura, Noto Sans) are compiled into guitarscore_bg.wasm, so
the editor works without fetching them; their licences are in assets/fonts/ in the source
tree and in THIRD_PARTY.md.

Optional files placed here are copied into the build:

  soundfonts/*.sf2   SoundFonts offered to the built-in synthesizer (M19; the browser
                     player is being built in `docs/web.md`).

icon.png is the app icon for the manifest and the page favicon.
