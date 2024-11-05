import pyarrow as pa

try:
    import pandas as pd
except ImportError:
    pass

try:
    import polars as pl
except ImportError:
    pass

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

    def sql_as_pandas(
        self,
        sql: str,
    ):
        df: pd.DataFrame = self.sql(sql).to_pandas()
        return df

    def sql_as_polars(
        self,
        sql: str,
    ):
        df: pl.DataFrame = pl.from_arrow(self.sql(sql))  # type: ignore
        return df

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

    def __repr__(self) -> str:
        return f'CdlFS({self._impl.path!r})'
