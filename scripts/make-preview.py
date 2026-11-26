"""Create an inert local .mrpack for the read-only native picker walkthrough."""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

root = Path(__file__).resolve().parents[1]
out = root / ".artifacts" / "samples" / "forest-preview.mrpack"
out.parent.mkdir(parents=True, exist_ok=True)
with ZipFile(out, "w") as pack:
    for name, data in [
        ("modrinth.index.json", (root / "fixtures" / "pack-index.json").read_bytes()),
        ("overrides/config/forest.txt", b"sample=true\n"),
    ]:
        entry = ZipInfo(name, (2020, 1, 1, 0, 0, 0))
        entry.compress_type = ZIP_DEFLATED
        entry.create_system = 3
        entry.external_attr = 0o100644 << 16
        pack.writestr(entry, data)
print(out)
