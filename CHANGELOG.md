# Changelog

## 0.1.2

### Features

* show the processing time per frame in parts, with its 95th and 99th percentile
* switch the whole filter on and off, kept for the next runs
* switch the whole filter on and off in the app
* show an icon in the tray that switches the filter and opens the app
* translate the app and the tray into German, Spanish, French, Russian, simplified Chinese and Japanese
* switch the echo cancellation from the tray's menu
* switch Listen from the tray's menu

### Fixes

* put the info buttons of the app at the end of their rows
* dim the stages in the app as soon as the filter is switched off

### Performance

* flush denormals to zero around every frame of the chain
* serve the noise suppression's allocations from a pool reserved at the start

### Dependencies

* update dependencies

## 0.1.1

### Features

* copy the status values of the app with a button, or select them
* keep the voice while talking into music from the speakers (Voice Protection)

### Fixes

* play Listen on a Bluetooth headset right away after it was idle
* line up the sliders of the app, all of the same width
* keep a stage's row collapsed when switching it on
* run the noise suppression fully on every frame, for clearer speech with less noise left
* keep the noise out of the speech pauses and a quiet voice in

### Performance

* read the status of the app with one D-Bus call a second instead of 15

### Dependencies

* update dependencies
