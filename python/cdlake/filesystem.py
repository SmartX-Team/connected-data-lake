import pyarrow as pa

from ._internal import CdlFS as _CdlFSImpl


class CdlFS:
    def __init__(self, impl: _CdlFSImpl) -> None:
        self._impl = impl

    def copy_to(self, dst: str) -> None:
        return self._impl.copy_to(dst)

    def read_dir(
        self,
        path: str,
    ) -> pa.RecordBatch:
        return self._impl.read_dir(path)

    def read_dir_all(self) -> pa.RecordBatch:
        return self._impl.read_dir_all()

    def sql(
        self,
        sql: str,
    ) -> pa.RecordBatch:
        return self._impl.sql(sql)

    def to_torch_dataset(
        self,
        batch_size: int = 1,
    ):
        from cdlake.dataset import CdlTorchDataset
        return CdlTorchDataset(
            batch=self.read_dir_all(),
            batch_size=batch_size,
            fs=self._impl,
        )
