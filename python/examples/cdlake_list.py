import argparse

import cdlake
import polars as pl


def main(src: str, path: str | None) -> None:
    cdl = cdlake.Cdl(
        max_cache_size=0,
    )
    fs = cdl.open(src)

    match path:
        case None:
            files = fs.read_dir_all()
        case path:
            files = fs.read_dir(path)
    print(pl.from_arrow(files))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('src', type=str, help='Source data directory')
    parser.add_argument('--path', type=str,
                        help='Destination data directory')

    args = parser.parse_args()
    main(args.src, args.path)
