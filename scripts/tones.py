# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy", "scipy", "soundfile"]
# ///
"""Steady tones in a recording (fans, electrical interference): where they are, how far above the
noise around them, how steady, in the pauses and in the speech; with docs/measurements.md.

    python scripts/tones.py in.wav [--remove out.wav] [--block 10]

Spectra of 8192 samples (5.9 Hz at 48 kHz) every 100 ms. A tone is a spectral peak more than
`--prominence` (12 dB) above the median within ±100 Hz, between 100 Hz and 12 kHz, listed if it's in
at least 20 % of the steps. A step is a pause where its level (the median of three) is less than
6 dB above the noise floor (the quietest step within ±1 s), speech where it's more than `--speech`
(10 dB) above it. The strongest tones of the pauses are followed over the whole file: their
frequency per `--block` seconds and how far it moves from step to step.

`--remove` writes a copy without the tones found in the pauses (notches of Q 30, run forwards and
backwards, so without a phase shift), to run through the chain against the original.
"""

import argparse

import numpy as np
import soundfile as sf
from scipy.ndimage import median_filter, minimum_filter1d
from scipy.signal import filtfilt, iirnotch, stft

NFFT = 8192
HOP = 4800  # 100 ms at 48 kHz


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("wav")
    parser.add_argument("--prominence", type=float, default=12, help="dB above the median around")
    parser.add_argument("--speech", type=float, default=10, help="dB above the noise floor")
    parser.add_argument("--top", type=int, default=16, help="tones listed at most")
    parser.add_argument("--block", type=float, default=10, help="seconds per frequency of a track")
    parser.add_argument("--remove", metavar="OUT", help="write a copy without the pause tones")
    args = parser.parse_args()

    x, sr = sf.read(args.wav, dtype="float32")
    if x.ndim > 1:
        x = x.mean(axis=1)
    f, _, spectrum = stft(x, sr, "hann", NFFT, NFFT - HOP, boundary=None, padded=False)
    # scipy divides by the window's sum: a full-scale sine comes out at 0 dB
    db = 10 * np.log10(np.abs(spectrum) ** 2 * 4 + 1e-20)
    df = f[1]

    steps = x[: len(x) // HOP * HOP].reshape(-1, HOP)
    n = min(len(steps), db.shape[1])
    db = db[:, :n]
    level = 10 * np.log10(np.mean(steps[:n] ** 2, axis=1) + 1e-20)
    floor = minimum_filter1d(level, 21)
    smooth = median_filter(level, 3)
    pause = smooth < floor + 6
    speech = smooth > floor + args.speech
    print(
        f"{args.wav}: {n / 10:.0f} s, pauses {pause.mean() * 100:.0f} %, speech {speech.mean() * 100:.0f} %, "
        f"pauses at {np.median(level[pause]):.1f} dBFS (median)"
    )

    prominence = db - median_filter(db, size=(int(200 / df) | 1, 1))
    band = (f > 100) & (f < 12000)
    peak = np.zeros_like(db, dtype=bool)
    peak[1:-1] = (db[1:-1] > db[:-2]) & (db[1:-1] >= db[2:]) & (prominence[1:-1] > args.prominence)
    peak &= band[:, None]

    def tones(mask, name):
        """The tones in the steps of `mask`, a row each, and their bins."""
        if not mask.any():
            return []
        # a tone that moves by a bin counts as one
        rate = np.convolve(peak[:, mask].mean(axis=1), np.ones(3), "same")
        found, taken = [], np.zeros(len(f), bool)
        for i in np.argsort(-rate):
            if rate[i] < 0.2 or len(found) >= args.top:
                break
            if not taken[max(0, i - 3) : i + 4].any():
                taken[i] = True
                found.append(i)
        print(f"  {name}, {mask.sum()} steps: tones in at least 20 % of them")
        print("        Hz  present  above around  level (dBFS)  5..95 % (Hz)")
        columns = np.nonzero(mask)[0]
        for i in sorted(found):
            lo, hi = max(0, i - 3), i + 4
            near = peak[lo:hi][:, mask]
            present = near.any(axis=0)
            where = np.argmax(np.where(near, db[lo:hi][:, mask], -np.inf), axis=0)[present] + lo
            at = columns[present]
            freqs = f[where]
            print(
                f"    {f[i]:6.0f}  {present.mean() * 100:5.0f} %  {np.median(prominence[where, at]):7.1f} dB"
                f"  {np.median(db[where, at]):12.1f}  {np.percentile(freqs, 5):6.0f}..{np.percentile(freqs, 95):.0f}"
            )
        return sorted(found)

    in_pauses = tones(pause, "pauses")
    tones(speech, "speech")

    strongest = sorted(in_pauses, key=lambda i: -np.median(prominence[i, pause]))[:4]
    block = max(1, int(args.block * 10))
    steps_n = np.arange(n)
    for i in sorted(strongest):
        reach = int(40 / df)
        lo = max(1, i - reach)
        at = np.argmax(db[lo : i + reach + 1], axis=0) + lo
        a, b, c = db[at - 1, steps_n], db[at, steps_n], db[at + 1, steps_n]
        freq = (at + 0.5 * (a - c) / (a - 2 * b + c + 1e-12)) * df  # parabolic interpolation
        freq = np.where(prominence[at, steps_n] > args.prominence, freq, np.nan)
        moves = np.abs(np.diff(freq))
        moves = moves[np.isfinite(moves)]
        print(
            f"  {f[i]:.0f} Hz (±40 Hz) in {np.isfinite(freq).mean() * 100:.0f} % of all steps, moving per step:"
            f" median {np.median(moves):.1f}, 95 % {np.percentile(moves, 95):.1f} Hz"
        )
        medians = []
        for start in range(0, n, block):
            part = freq[start : start + block]
            enough = np.isfinite(part).sum() > len(part) // 10
            medians.append(f"{np.nanmedian(part):.0f}" if enough else "-")
        print(f"    per {args.block:g} s: " + " ".join(medians))

    if args.remove:
        y = x.astype(np.float64)
        for i in in_pauses:
            b, a = iirnotch(f[i], 30, sr)
            y = filtfilt(b, a, y)
        sf.write(args.remove, y.astype(np.float32), sr, subtype="FLOAT")
        print(f"  wrote {args.remove} without " + ", ".join(f"{f[i]:.0f}" for i in in_pauses) + " Hz")


if __name__ == "__main__":
    main()
