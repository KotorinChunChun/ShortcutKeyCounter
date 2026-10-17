"""Cargo metadataからWindows向けのライセンス台帳を再生成する。Python 3.11以上。"""
import argparse
import hashlib
import json
import re
import shutil
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('metadata', type=Path)
args = parser.parse_args()
metadata = json.loads(args.metadata.read_text(encoding='utf-8-sig'))
checksums = {(p['name'], p['version']): p.get('checksum') for p in
             tomllib.loads((root / 'Cargo.lock').read_text(encoding='utf-8'))['package']}
out = root / 'third-party'
out.mkdir(exist_ok=True)
records = []
texts = ['第三者ソフトウェアのライセンス・著作権表示\n\n'
         '対象: x86_64-pc-windows-msvc。ビルド用・開発用依存も保守的に含む。\n'
         '各コンポーネントには個別のライセンスが適用され、本体MITの一括適用はしない。\n'
         'MPL-2.0の未改変ソースはsources/に同梱し、各配布元URLでも取得できる。\n']
for p in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
    if not p['source']:
        continue
    folder = Path(p['manifest_path']).parent
    key = p['name'] + '-' + p['version']
    candidates = sorted(f for f in folder.rglob('*') if f.is_file() and
                        re.match(r'^(licen[sc]e|copying|notice|copyright)([-._]|$)', f.name, re.I))
    extra = out / 'upstream' / key
    if extra.exists():
        candidates += sorted(extra.glob('*.txt'))
    if not candidates or not p['license']:
        raise SystemExit('ライセンス本文または宣言が未確認: ' + key)
    source = f"https://crates.io/api/v1/crates/{p['name']}/{p['version']}/download"
    texts.append(f"\n{'=' * 72}\n{key}\nSPDX: {p['license']}\nSource: {source}\n")
    licenses = []
    for f in candidates:
        relative = f.relative_to(folder).as_posix() if f.is_relative_to(folder) else 'upstream/' + f.name
        data = f.read_bytes()
        texts.append(f'\n--- {relative} ---\n' + data.decode('utf-8-sig') + '\n')
        licenses.append({'file': relative, 'sha256': hashlib.sha256(data).hexdigest()})
    record = {'name': p['name'], 'version': p['version'], 'license': p['license'],
              'source': source, 'repository': p['repository'], 'notices': licenses}
    if 'MPL-2.0' in p['license']:
        # Cargoキャッシュの原本を同梱し、改変せず取得できるようにする。
        # registry/src/index/name から registry/cache/index/name を解決
        archive = folder.parent.parent.parent / 'cache' / folder.parent.name / (key + '.crate')
        if not archive.is_file():
            raise SystemExit('MPL原本アーカイブがありません: ' + key)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != checksums[(p['name'], p['version'])]:
            raise SystemExit('MPL原本とCargo.lockのchecksumが不一致: ' + key)
        destination = out / 'sources' / archive.name
        destination.parent.mkdir(exist_ok=True)
        shutil.copyfile(archive, destination)
        record['bundledSource'] = 'sources/' + archive.name
        record['sourceSha256'] = hashlib.sha256(archive.read_bytes()).hexdigest()
    records.append(record)
(out / 'THIRD-PARTY-NOTICES.txt').write_text(''.join(texts).rstrip() + '\n', encoding='utf-8')
manifest = {'target': 'x86_64-pc-windows-msvc',
            'cargoLockSha256': hashlib.sha256((root / 'Cargo.lock').read_bytes()).hexdigest(),
            'packages': records}
(out / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(f'ライセンス本文収録: {len(records)} packages')
