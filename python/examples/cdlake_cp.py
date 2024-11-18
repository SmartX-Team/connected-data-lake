import argparse

import cdlake


def main(src: str, dst: str) -> None:
    cdl = cdlake.Cdl(
        max_cache_size=0,
    )
    fs = cdl.open(src)
    fs.copy_to(dst)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('src', type=str, help='Source data directory')
    parser.add_argument('dst', type=str, help='Destination data directory')

    args = parser.parse_args()
    main(args.src, args.dst)
