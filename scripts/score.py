# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy", "scipy", "soundfile", "pesq", "pystoi"]
# ///
"""Scores processed versions of a test set against its clean files: PESQ (wideband), STOI and
SI-SDR, for each directory and by the SNR of the noisy input, see docs/measurements.md.

    python scripts/score.py CLEAN NOISY OUT [OUT ...] [--start 1.0]

CLEAN and NOISY hold the test set's clean and noisy files, each OUT what a variant made of the
noisy ones (examples/batch.rs), with the same names, lined up with the input. The noisy files are
scored too, as the baseline. SI-SDR says little after an IIR high-pass, which shifts the phase
against the reference.
"""

import argparse
import os
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from pesq import pesq
from pystoi import stoi
from scipy.signal import resample_poly

# the bands of the noisy input's SI-SDR the scores are split by, in dB
BANDS = [(-np.inf, 5), (5, 10), (10, 15), (15, np.inf)]
BAND_NAMES = ["< 5", "5–10", "10–15", "> 15"]


def si_sdr(reference, estimate):
    reference = reference - reference.mean()
    estimate = estimate - estimate.mean()
    target = np.dot(estimate, reference) / np.dot(reference, reference) * reference
    return 10 * np.log10(np.sum(target**2) / (np.sum((estimate - target) ** 2) + 1e-12))


def score(task):
    """PESQ, STOI and SI-SDR of one file in every directory."""
    name, clean_dir, dirs, start = task
    clean, rate = sf.read(os.path.join(clean_dir, name), dtype="float32")
    first = int(start * rate)
    clean = clean[first:]
    scores = []
    for directory in dirs:
        processed, _ = sf.read(os.path.join(directory, name), dtype="float32")
        processed = processed[first:]
        n = min(len(clean), len(processed))
        reference, estimate = clean[:n], processed[:n]
        try:
            quality = pesq(16000, resample_poly(reference, 1, 3), resample_poly(estimate, 1, 3), "wb")
        except Exception:  # too short or silent: no PESQ
            quality = np.nan
        scores.append((quality, stoi(reference, estimate, rate), si_sdr(reference, estimate)))
    return scores


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("clean")
    parser.add_argument("noisy")
    parser.add_argument("outputs", nargs="+")
    parser.add_argument("--start", type=float, default=0.0, help="seconds to leave out at the start")
    args = parser.parse_args()

    dirs = [args.noisy] + args.outputs
    labels = ["noisy"] + [os.path.basename(os.path.normpath(d)) for d in args.outputs]
    names = sorted(n for n in os.listdir(args.clean) if n.endswith(".wav"))
    with Pool() as pool:
        scores = np.array(pool.map(score, [(n, args.clean, dirs, args.start) for n in names]))
    # files × directories × (PESQ, STOI, SI-SDR)
    width = max(len(label) for label in labels)
    print(f"{len(names)} files")
    print(f"{'':{width}}  {'PESQ':>5}  {'STOI':>5}  {'SI-SDR':>9}  PESQ by the input's SI-SDR (" + ", ".join(BAND_NAMES) + " dB)")
    bands = [(scores[:, 0, 2] >= lo) & (scores[:, 0, 2] < hi) for lo, hi in BANDS]
    for d, label in enumerate(labels):
        pesq_mean, stoi_mean, sdr_mean = np.nanmean(scores[:, d, 0]), scores[:, d, 1].mean(), scores[:, d, 2].mean()
        by_band = ", ".join(f"{np.nanmean(scores[band, d, 0]):.2f}" for band in bands)
        print(f"{label:{width}}  {pesq_mean:5.2f}  {stoi_mean:5.3f}  {sdr_mean:6.2f} dB  {by_band}")
    counts = ", ".join(f"{name} dB: {band.sum()}" for name, band in zip(BAND_NAMES, bands))
    print(f"files by the input's SI-SDR: {counts}")


if __name__ == "__main__":
    main()
