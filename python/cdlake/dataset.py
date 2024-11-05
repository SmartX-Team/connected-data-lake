from cdlake._internal import CdlFS
import pyarrow as pa
import torch


class CdlTorchDataset(torch.utils.data.Dataset[torch.Tensor]):
    def __init__(
        self,
        batch: pa.RecordBatch,
        batch_size: int,
        fs: CdlFS,
    ) -> None:
        super().__init__()
        self._batch = batch
        self._batch_size = batch_size
        self._fs = fs

    def __getitem__(
        self,
        index: int,
    ) -> torch.Tensor:
        return self.__getitems__([index])[0]

    def __getitems__(
        self,
        indices: list[int],
    ) -> list[torch.Tensor]:
        # FIXME: use indices directly
        df = self._batch.take(indices)
        parents = df['parent']
        names = df['name']

        conditions = []
        for parent, name in zip(parents, names):
            conditions.append(
                f'parent LIKE {str(parent)!r} AND name LIKE {str(name)!r}',
            )

        return [
            torch.Tensor(file_data)
            for file_data in self._fs.read_files(
                condition=' OR '.join(conditions),
            )
        ]

    def __len__(self) -> int:
        return len(self._batch)
