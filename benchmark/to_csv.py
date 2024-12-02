#!/usr/bin/env python3


import json
from pathlib import Path

import pandas as pd


def _main(
    output_dir=Path('./outputs'),
) -> None:
    if not output_dir.is_dir():
        raise FileNotFoundError('No output directory')

    files = sorted(output_dir.glob('*.json'))
    if not files:
        return

    ds = []
    for file in files:
        if not file.is_file():
            continue
        with file.open('r+b') as fp:
            ds.append(json.load(fp))

    df = pd.DataFrame(ds)
    df.to_csv(f'{files[-1].name[:-5]}.csv')


if __name__ == '__main__':
    _main()
