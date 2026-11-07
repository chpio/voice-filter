# Changelog

## 0.1.5

### Features

* add a Lithuanian translation

### Fixes

* stop writing into freed memory when Listen is switched off or the daemon stops while listening
* keep the saved settings in step when the app and the tray change them at once, and start with the defaults when a saved setting can't be read
* move the sliders in the app in their steps, and show a reset or a change from elsewhere on them at once, even while one has the focus

## 0.1.4

### Features

* keep more of the voice while you talk into the playback, with the echo cancellation tuned for it and Voice Protection off by default

### Dependencies

* update dependencies

## 0.1.3

### Fixes

* keep the output from turning to NaN after 78 s of digital silence
* keep the tray and the app from saving the settings over each other

### Dependencies

* update dependencies

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
