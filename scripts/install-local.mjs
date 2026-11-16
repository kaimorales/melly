import { execFileSync } from 'node:child_process';
import { lstat, mkdtemp, mkdir, realpath, rename, rm, symlink } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cargoTargetDir } from './rust-env.mjs';

export const builtApp = join(cargoTargetDir(), 'release/bundle/macos/Melly.app');
export const installedApp = '/Applications/Melly.app';

async function present(path) {
  try { return await lstat(path); } catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}

export async function inspectBundle(path) {
  const stat = await lstat(path);
  if (!stat.isDirectory() || stat.isSymbolicLink() || basename(path) !== 'Melly.app') throw new Error('Choose the regular Melly.app bundle.');
  const plist = join(path, 'Contents/Info.plist');
  const read = field => execFileSync('/usr/libexec/PlistBuddy', ['-c', `Print :${field}`, plist], { encoding: 'utf8' }).trim();
  if (read('CFBundleIdentifier') !== 'com.melly.desktop' || read('CFBundleExecutable') !== 'melly-desktop') throw new Error('Only the production Melly app can be installed or replaced.');
  execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', path], { stdio: 'pipe' });
  return read('CFBundleShortVersionString');
}

function isRunning() {
  try { return !!execFileSync('/usr/bin/pgrep', ['-x', 'melly-desktop'], { encoding: 'utf8' }).trim(); }
  catch (error) {
    if (error.status === 1 && !String(error.stderr || '').trim()) return false;
    throw new Error('Could not check whether Melly is running. Quit Melly and retry outside the restricted runner.');
  }
}

function copyBundle(source, destination) {
  execFileSync('/usr/bin/ditto', [source, destination], { stdio: 'pipe' });
}

export async function installBundle({ source, destination, inspect = inspectBundle, copy = copyBundle, running = isRunning, promote = rename }) {
  source = resolve(source); destination = resolve(destination);
  if (basename(source) !== 'Melly.app' || basename(destination) !== 'Melly.app') throw new Error('The app must retain the name Melly.app.');
  if (source === destination) throw new Error('Source and installed app must be different locations.');
  if (await running()) throw new Error('Quit Melly before installing an update.');
  const version = await inspect(source);
  const existing = await present(destination);
  if (existing) await inspect(destination);
  await mkdir(dirname(destination), { recursive: true });
  const parent = await realpath(dirname(destination));
  const sourcePath = await realpath(source);
  if (parent === sourcePath || parent.startsWith(sourcePath + '/')) throw new Error('The destination cannot be inside the source app.');
  destination = join(parent, 'Melly.app');
  if (destination === sourcePath) throw new Error('Source and installed app must be different locations.');
  const transaction = await mkdtemp(join(parent, '.melly-install-'));
  const staged = join(transaction, 'Melly.app');
  const previous = join(transaction, 'previous');
  let backedUp = false;
  let committed = false;
  let keepRecovery = false;
  try {
    await copy(source, staged);
    await inspect(staged);
    if (existing) { await rename(destination, previous); backedUp = true; }
    try {
      await promote(staged, destination);
      committed = true;
    } catch (error) {
      if (backedUp) {
        try { await rename(previous, destination); backedUp = false; }
        catch { keepRecovery = true; throw new Error(`Installation failed. The previous app is preserved at ${previous}.`, { cause: error }); }
      }
      throw error;
    }
  } finally {
    if (!keepRecovery) await rm(transaction, { recursive: true, force: true });
  }
  return { destination, version, committed };
}

export async function linkLauncher(app, bin) {
  await mkdir(bin, { recursive: true });
  const path = join(bin, 'melly');
  const target = join(app, 'Contents/MacOS/melly-desktop');
  const existing = await present(path);
  if (existing) {
    if (existing.isSymbolicLink() && await realpath(path) === await realpath(target)) return path;
    throw new Error(`The app is installed, but ${path} already belongs to another command. It was left in place.`);
  }
  await symlink(target, path);
  return path;
}

async function main() {
  if (process.argv.length > 2) throw new Error('Run npm run install:local without arguments.');
  if (process.platform !== 'darwin') throw new Error('Melly is a macOS app.');
  if (['/opt/homebrew/Caskroom/melly', '/usr/local/Caskroom/melly'].some(existsSync)) throw new Error('Homebrew owns this installation. Use melly --update instead of replacing its files manually.');
  const result = await installBundle({ source: builtApp, destination: installedApp });
  // Only consume this checkout’s known build artifact after successful promotion.
  await rm(builtApp, { recursive: true });
  console.log(`Installed Melly ${result.version} at ${result.destination}. Chats, project files and credentials stay in their existing profile.`);
  const bin = join(homedir(), '.local/bin');
  const launcher = await linkLauncher(result.destination, bin);
  console.log(`Launcher: ${launcher}`);
  if (!(process.env.PATH || '').split(':').includes(bin)) console.log(`Add ${bin} to your shell PATH to use the melly command. Homebrew will provide its own PATH link when the cask is available.`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
