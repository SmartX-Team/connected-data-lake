import argparse

import cdlake
import torch


def main(src: str) -> None:
    cdl = cdlake.Cdl(
        max_cache_size=0,
    )
    fs = cdl.open(src)

    loader = torch.utils.data.DataLoader(
        dataset=fs.to_torch_dataset(),
        batch_size=1,
    )
    for index, data in zip(range(4), loader):
        print(f'{index:06d} => {data.shape}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('src', type=str, help='Source data directory')

    args = parser.parse_args()
    main(args.src)
