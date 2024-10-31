import pyarrow as pa

try:
    import torch
except ImportError:
    pass

from ._internal import CdlFS as _CdlFSImpl


class CdlTorchDataset(torch.utils.data.Dataset[bytes]):
    def __init__(
        self,
        batch: pa.RecordBatch,
        batch_size: int,
        fs: 'CdlFS',
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
        return self._fs._impl.read_files(self._batch.take(indices))

    def __len__(self) -> int:
        return len(self._batch)


class CdlFS:
    def __init__(self, impl: _CdlFSImpl, /) -> None:
        self._impl = impl

    def copy_to(self, dst: str, /) -> None:
        return self._impl.copy_to(dst)

    def to_torch_dataset(
        self,
        batch_size: int = 1,
    ) -> CdlTorchDataset:
        return CdlTorchDataset(
            batch=self._impl.read_dir_all(),
            batch_size=batch_size,
            fs=self,
        )
