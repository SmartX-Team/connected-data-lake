from typing import Any

import pyarrow as pa


class CdlFS:
    dataset_uri: str
    global_path: str

    def copy_to(self, dst: str, /) -> None: ...

    def read_dir(self, path: str = '/', /) -> pa.RecordBatch: ...

    def read_dir_all(self, /) -> pa.RecordBatch: ...

    def read_files(self, /, condition: str) -> list[bytes]: ...

    def sql(self, sql: str, /) -> pa.RecordBatch: ...

    def storage_options(self) -> dict[str, str]: ...


class Cdl:
    def __init__(self, catalog: dict[str, Any], /) -> None: ...

    def open(self, url: str, /) -> CdlFS: ...
