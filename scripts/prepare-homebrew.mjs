import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { basename, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function renderCask(template, version, sha256) {
  if (!/^\d+\.\d+\.\d+$/.test(version) || !/^[a-f0-9]{64}$/.test(sha256)) throw new Error('Use a stable semantic version and the archive’s SHA-256.');
  if (!template.includes('@VERSION@') || !template.includes('@SHA256@')) throw new Error('Invalid cask template.');
  return template.replaceAll('@VERSION@', version).replaceAll('@SHA256@', sha256);
}

async function main() {
  if (process.argv.length !== 3) throw new Error('Usage: npm run prepare:homebrew -- /path/to/Melly-VERSION-macos-arm64.zip');
  const archive = resolve(process.argv[2]);
  const base = new URL('../', import.meta.url);
  const packageData = JSON.parse(await readFile(new URL('package.json', base), 'utf8'));
  const config = JSON.parse(await readFile(new URL('src-tauri/tauri.conf.json', base), 'utf8'));
  const cargo = await readFile(new URL('src-tauri/Cargo.toml', base), 'utf8');
  const version = packageData.version;
  if (config.version !== version || cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1] !== version) throw new Error('Synchronize package.json, Cargo.toml and tauri.conf.json versions before packaging.');
  if (basename(archive) !== `Melly-${version}-macos-arm64.zip`) throw new Error('The archive name must match the version and architecture in the cask.');
  const meta = await stat(archive);
  if (!meta.isFile() || meta.size === 0) throw new Error('Choose the actual release archive.');
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(archive)) hash.update(chunk);
  const template = await readFile(new URL('packaging/homebrew/melly.rb.in', base), 'utf8');
  const cask = renderCask(template, version, hash.digest('hex'));
  const folder = new URL('target/homebrew/', base); await mkdir(folder, { recursive: true });
  const destination = new URL('melly.rb', folder); await writeFile(destination, cask);
  console.log(`Prepared ${fileURLToPath(destination)}. Nothing was published. Verify Developer ID signing/notarization and the release URL before placing this in kaimorales/homebrew-tap.`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch(error => { console.error(error.message); process.exitCode = 1; });
