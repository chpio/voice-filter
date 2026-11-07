# Changelog

## 0.2.0

### Features

* remove the helpers the real-time inference doesn't use

### Fixes

* keep the output from turning to NaN after 78 s of digital silence
* apply the post filter to the highest frequency bin too
* pass quiet input through at a limit of 0 dB instead of muting it

## 0.1.1

### Features

* analyze a frame without enhancing it, and apply the gains to another signal

### Fixes

* write the normalized spectrum in band_unit_norm_t instead of dividing the output buffer

### Performance

* keep the noise suppression's own code free of allocations
