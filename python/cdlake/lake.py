from ._internal import Cdl as _CdlImpl
from .filesystem import CdlFS


class Cdl:
    def __init__(self, **catalog) -> None:
        self._impl = _CdlImpl(catalog)

    def open(self, url: str) -> CdlFS:
        return CdlFS(self._impl.open(url))

    def __repr__(self) -> str:
        return 'Connected Data Lake'
