import lance
import pyarrow as pa

try:
    import lance.torch.data
except ImportError:
    pass

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
    ) -> pd.DataFrame:
        df: pd.DataFrame = self.sql(sql).to_pandas()
        return df

    def sql_as_polars(
        self,
        sql: str,
    ) -> pl.DataFrame:
        df: pl.DataFrame = pl.from_arrow(self.sql(sql))  # type: ignore
        return df

    def to_lance_dataset(self, **kwargs) -> lance.LanceDataset:
        return lance.LanceDataset(
            storage_options=self._impl.storage_options(),
            uri=self._impl.dataset_uri,
            **kwargs,
        )

    def to_torch_dataset(
        self,
        batch_size: int = 1,
        **kwargs,
    ) -> lance.torch.data.LanceDataset:
        return lance.torch.data.LanceDataset(
            batch_size=batch_size,
            dataset=self.to_lance_dataset(),
            **kwargs,
        )

    def __repr__(self) -> str:
        return f'CdlFS({self._impl.global_path!r})'
