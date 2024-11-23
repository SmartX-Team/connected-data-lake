import argparse
import time

import cdlake
import numpy as np
import pyarrow as pa


def main(src: str) -> None:
    print('Initializing Connected Data Lake...')
    cdl = cdlake.Cdl(
        max_cache_size=0,
    )

    print('Opening a dataset...')
    fs = cdl.open(src)

    print('Getting a lance table...')
    ds = fs.to_lance_dataset()

    print('Testing random access across dataset...')
    indices = np.random.choice(
        a=np.arange(len(ds)),
        size=min(len(ds), 100),
        replace=False,
    )
    for size in range(1, len(indices) + 1):
        start = time.time()
        _: pa.Table = ds.take(
            indices=indices[:size],
            columns=['parent', 'name', 'data'],
        )
        end = time.time()
        print(f'{size} random rows takes {end - start} seconds.')
    print('Finalizing Connected Data Lake...')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('src', type=str, help='Source data directory')

    args = parser.parse_args()
    main(args.src)
