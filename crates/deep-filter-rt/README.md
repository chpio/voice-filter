# deep-filter-rt

Real-time noise suppression with [DeepFilterNet3](https://github.com/Rikorose/DeepFilterNet), in
pure Rust on the CPU. It's the inference part of DeepFilterNet's `deep_filter` crate, which is only
usable from git (the last release on crates.io is from 2022), reduced to that, updated to current
dependencies and with the DeepFilterNet3 model embedded. See [NOTICE.md](NOTICE.md) for its origin
and the changes.

It processes 48 kHz audio in frames of 10 ms (480 samples per channel):

```rust
use deep_filter_rt::tract::DfTract;
use ndarray::Array2;

let mut df = DfTract::default(); // the embedded model with the default parameters
let noisy = Array2::<f32>::zeros((df.ch, df.hop_size));
let mut enhanced = Array2::<f32>::zeros((df.ch, df.hop_size));
let lsnr = df.process(noisy.view(), enhanced.view_mut())?; // local SNR estimate in dB
```

Debug builds run the model a lot slower than release builds.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.
