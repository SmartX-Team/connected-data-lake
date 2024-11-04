from enum import Enum

import pyarrow as pa


class Compression(Enum):
    BROTLI = 0
    GZIP = 1
    LZO = 2
    LZ4 = 3
    LZ4_RAW = 4
    SNAPPY = 5
    UNCOMPRESSED = 6
    ZSTD = 7


class Url:
    def __init__(self, url: str, /) -> None: ...


class DatasetCatalog:
    compression: Compression
    compression_level: int | None = None
    max_buffer_size: int
    max_chunk_size: int
    max_write_threads: int
    s3_access_key: str | None = None
    s3_endpoint: Url
    s3_region: str
    s3_secret_key: str | None = None


class CdlFS:
    def copy_to(self, dst: str, /) -> None: ...

    def read_dir(self, path: str = '/', /) -> pa.RecordBatch: ...

    def read_dir_all(self, /) -> pa.RecordBatch: ...

    def read_files(self, files: pa.RecordBatch, /) -> list[bytes]: ...

    def sql(self, sql: str, /) -> pa.RecordBatch: ...


class Cdl:
    def __init__(
        self, /,
        catalog: DatasetCatalog | None = None,
    ) -> None: ...

    def open(self, url: str, /) -> CdlFS: ...
