// Changes the app's version number everywhere it's written.
// Usage: node scripts/set-version.mjs 0.2.0
import fs from 'node:fs';

const v = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(v ?? '')) {
  console.error('Usage: node scripts/set-version.mjs 1.2.3');
  process.exit(1);
}

function edit(file, pattern) {
  const text = fs.readFileSync(file, 'utf8');
  if (!pattern.test(text)) {
    console.error(`Couldn't find the version in ${file}`);
    process.exit(1);
  }
  fs.writeFileSync(file, text.replace(pattern, `$1${v}$2`));
  console.log(`${file} -> ${v}`);
}

edit('package.json', /("version":\s*")[^"]+(")/);
edit('src-tauri/tauri.conf.json', /("version":\s*")[^"]+(")/);
edit('src-tauri/Cargo.toml', /(\[package\][\s\S]*?\nversion\s*=\s*")[^"]+(")/);
