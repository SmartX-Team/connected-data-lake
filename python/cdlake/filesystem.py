try:
    import torch
except ImportError:
    pass

from ._internal import CdlFS as _CdlFSImpl


class CdlDataset[T](torch.utils.data.IterableDataset[T]):
    def __getitem__(self, index: int) -> T:
        raise NotImplementedError(
            "Subclasses of Dataset should implement __getitem__.")

    def __iter__(self):
        return iter([])

    def __len__(self) -> int:
        return 0


class CdlFS:
    def __init__(self, impl: _CdlFSImpl, /) -> None:
        self._impl = impl

    def copy_to(self, dst: str, /) -> None:
        return self._impl.copy_to(dst)

    def to_torch_dataset[T](self) -> CdlDataset:
        self._impl.read_dir('/')
        return CdlDataset[T]()
