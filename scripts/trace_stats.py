# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy"]
# ///
"""Noise in the pauses and lost speech of a recording, from traces of examples/trace.rs with
`--interval 0.01`, with the definitions of docs/measurements.md.

    python scripts/trace_stats.py REFERENCE.tsv [OTHER.tsv ...] [--events 10]

REFERENCE is the trace the pauses and the speech are taken from (its input) and what the others
lose speech against, e.g. the chain as it is; the others are variants on the same recording. A
frame is a pause where the median input level within ±150 ms is less than 8 dB above the noise
floor (the quietest frame within ±1 s), speech where it's more than 20 dB above it. Noise in a
pause: an output frame above −60 dBFS; speech lost: a speech frame that comes out below −45 dBFS
while the reference keeps it above −30 dBFS.
"""

import argparse

import numpy as np
from numpy.lib.stride_tricks import sliding_window_view

PAUSE_ABOVE_FLOOR_DB = 8.0
SPEECH_ABOVE_FLOOR_DB = 20.0
NOISE_DB = -60.0
LOST_DB = -45.0
KEPT_DB = -30.0


def load(path):
    """The columns of a trace by name, and the name of the output's column (the last stage)."""
    with open(path) as file:
        header = file.readline().rstrip("\n").split("\t")
        rows = [line.rstrip("\n").split("\t") for line in file]
    columns = {
        name: np.array([float(row[i]) if row[i] else np.nan for row in rows])
        for i, name in enumerate(header)
    }
    levels = [name for name in header[2:] if "." not in name]
    return columns, levels[-1]


def rolling(function, values, width):
    pad = np.pad(values, (width // 2, width - 1 - width // 2), mode="edge")
    return function(sliding_window_view(pad, width), axis=1)


def events(mask, levels, times, count):
    """The loudest runs of `mask` (gaps under 100 ms merged): start, length, peak."""
    runs = []
    for i in np.flatnonzero(mask):
        if runs and i - runs[-1][1] <= 10:
            runs[-1][1] = i
        else:
            runs.append([i, i])
    runs.sort(key=lambda run: -levels[run[0] : run[1] + 1].max())
    return [
        f"{times[a] // 60:.0f}:{times[a] % 60:05.2f} ({(b - a + 1) * 10} ms, {levels[a:b + 1].max():.0f} dBFS)"
        for a, b in runs[:count]
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("reference")
    parser.add_argument("others", nargs="*")
    parser.add_argument("--events", type=int, default=0, help="list the loudest noise in pauses")
    args = parser.parse_args()

    reference, output = load(args.reference)
    level = reference["input"]
    floor = rolling(np.min, level, 200)
    median = rolling(np.median, level, 31)
    pause = median < floor + PAUSE_ABOVE_FLOOR_DB
    speech = median > floor + SPEECH_ABOVE_FLOOR_DB
    kept = reference[output] > KEPT_DB
    print(f"{len(level)} frames: pauses {pause.mean() * 100:.0f} %, speech {speech.mean() * 100:.0f} %")
    names = [args.reference] + args.others
    width = max(len(name) for name in names)
    print(f"{'':{width}}  {'noise in pauses':>15}  {'median':>6}  {'estimate > 0 dB':>15}  {'speech out':>10}  {'speech lost':>11}")
    for name in names:
        trace, out = load(name)
        n = min(len(trace[out]), len(level))
        levels, p, s = trace[out][:n], pause[:n], speech[:n]
        snr = trace.get("denoise.snr", np.full(n, np.nan))[:n]
        lost = s & (levels < LOST_DB) & kept[:n]
        print(
            f"{name:{width}}  {np.mean(levels[p] > NOISE_DB) * 100:14.1f}%  {np.median(levels[p]):6.1f}  "
            f"{np.mean(snr[p] > 0) * 100:14.1f}%  {np.median(levels[s]):10.1f}  {lost.sum() / s.sum() * 100:10.1f}%"
        )
        if args.events:
            noisy = p & (levels > NOISE_DB)
            print(f"{'':{width}}  loudest noise: " + ", ".join(events(noisy, levels, trace["time"], args.events)))


if __name__ == "__main__":
    main()
