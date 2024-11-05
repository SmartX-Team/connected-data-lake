import argparse

import cdlake


def main(src: str, sql: str) -> None:
    cdl = cdlake.Cdl()
    fs = cdl.open(src)

    df = fs.sql_as_polars(sql)
    print(df)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('src', type=str, help='Source data directory')
    parser.add_argument('sql', type=str, help='A query to execute')

    args = parser.parse_args()
    main(args.src, args.sql)
