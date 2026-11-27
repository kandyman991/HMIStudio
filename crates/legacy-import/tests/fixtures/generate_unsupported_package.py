"""Generate the original synthetic transport-valid, portable-invalid fixture."""
import hashlib
import io
import json
from pathlib import Path
import zipfile

project = b'{"schemaVersion":1,"metadata":{"name":"Synthetic unsupported","defaultScreenId":"main"},"screens":{"main":{"id":"main","title":"Main","widthDp":640,"heightDp":480,"components":{}}},"variables":{},"externalSources":{},"externalMappings":{},"syntheticUnsupported":true}'
settings = b'{"values":"SIMULATED","fullScreen":true,"statusBar":true,"keepScreenOn":true}'
files = [
    dict(path=name, sha256=hashlib.sha256(data).hexdigest(), sizeBytes=len(data))
    for name, data in [("project.json", project), ("runtime.json", settings)]
]
identity = "\n".join(f'{f["path"]}:{f["sha256"]}:{f["sizeBytes"]}' for f in files)
manifest = dict(
    format=1, packageId=hashlib.sha256(identity.encode()).hexdigest(),
    projectName="Synthetic unsupported", version="1", schemaVersion=1,
    createdAtMs=1, createdBy="synthetic", startupScreen="main", files=files,
)
buf = io.BytesIO()
with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_STORED) as archive:
    for name, data in [
        ("manifest.json", json.dumps(manifest, separators=(",", ":")).encode()),
        ("project.json", project), ("runtime.json", settings),
    ]:
        entry = zipfile.ZipInfo(name)
        entry.create_system = 3
        entry.external_attr = 0o100600 << 16
        archive.writestr(entry, data)
Path(__file__).with_name("runtime-unsupported.synthetic").write_bytes(buf.getvalue())
