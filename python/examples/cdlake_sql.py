import argparse

import cdlake
import polars as pl


def main(src: str, sql: str) -> None:
    cdl = cdlake.Cdl()
    fs = cdl.open(src)

    df = fs.sql(sql)
    print(pl.from_arrow(df))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('src', type=str, help='Source data directory')
    parser.add_argument('sql', type=str, help='A query to execute')

    args = parser.parse_args()
    main(args.src, args.sql)
