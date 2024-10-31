from enum import Enum


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
    def __init__(self, /, url: str) -> None: ...


class DatasetCatalog:
    ...


class FileMetadataRecord:
    ...


class FileRecordRef:
    name: str
    parent: str | None
    metadata: FileMetadataRecord | None
    chunk_id: int
    chunk_offset: int
    chunk_size: int


class FileRecord(FileRecordRef):
    ...


class CdlFS:
    def copy_to(self, dst: str) -> None: ...


class Cdl:
    def __init__(
        self, /,
        catalog: DatasetCatalog | None = None,
    ) -> None: ...

    def open(self, url: str) -> CdlFS: ...
