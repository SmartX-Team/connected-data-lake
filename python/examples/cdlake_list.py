import argparse

import cdlake


def main(src: str) -> None:
    cdl = cdlake.Cdl()
    fs = cdl.open(src)
    ds = fs.to_torch_dataset()
    print(list(ds))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('src', type=str, help='Source data directory')

    args = parser.parse_args()
    main(args.src)
