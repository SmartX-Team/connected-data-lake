from cdlake._internal import CdlFS
import pyarrow as pa
import torch


class CdlTorchDataset(torch.utils.data.Dataset[bytes]):
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
    ) -> bytes:
        return self.__getitems__([index])[0]

    def __getitems__(
        self,
        indices: list[int],
    ) -> list[bytes]:
        print(self._batch.take(indices))
        return self._fs.read_files(self._batch.take(indices))

    def __len__(self) -> int:
        return len(self._batch)
