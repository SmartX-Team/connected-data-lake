from ._internal import (Cdl as _CdlImpl, DatasetCatalog)
from .filesystem import CdlFS


class Cdl:
    def __init__(
        self,
        catalog: DatasetCatalog | None = None,
    ) -> None:
        self._impl = _CdlImpl(catalog)

    def open(self, url: str) -> CdlFS:
        return CdlFS(self._impl.open(url))
