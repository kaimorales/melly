import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cp, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cargoTargetDir } from './rust-env.mjs';
import { installBundle, linkLauncher } from './install-local.mjs';
import { renderCask } from './prepare-homebrew.mjs';

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'melly-install-test-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const source = join(root, 'build/Melly.app'); const destination = join(root, 'Applications/Melly.app');
  for (const [path, version] of [[source, 'new'], [destination, 'old']]) {
    await mkdir(join(path, 'Contents/MacOS'), { recursive: true });
    await writeFile(join(path, 'Contents/MacOS/melly-desktop'), version);
  }
  const inspect = path => readFile(join(path, 'Contents/MacOS/melly-desktop'), 'utf8');
  const options = { source, destination, inspect, copy: (a, b) => cp(a, b, { recursive: true }), running: () => false };
  return { root, ...options, options };
}

test('updates one app and preserves an independent profile', async t => {
  const f = await fixture(t); const profile = join(f.root, 'profile');
  await mkdir(profile); await writeFile(join(profile, 'synthetic-data'), 'retain');
  await installBundle(f.options);
  assert.equal(await f.inspect(f.destination), 'new');
  assert.deepEqual(await readdir(join(f.root, 'Applications')), ['Melly.app']);
  assert.equal(await readFile(join(profile, 'synthetic-data'), 'utf8'), 'retain');
});

test('failed promotion restores the previous app', async t => {
  const f = await fixture(t);
  await assert.rejects(installBundle({ ...f.options, promote: () => { throw new Error('Synthetic failure'); } }), /Synthetic failure/);
  assert.equal(await f.inspect(f.destination), 'old');
  assert.deepEqual(await readdir(join(f.root, 'Applications')), ['Melly.app']);
});

test('running apps and invalid staged copies are never replaced', async t => {
  const f = await fixture(t);
  await assert.rejects(installBundle({ ...f.options, running: () => true }), /Quit Melly/);
  await assert.rejects(installBundle({ ...f.options, inspect: async path => { if (path.includes('.melly-install-')) throw new Error('Invalid signature'); return f.inspect(path); } }), /Invalid signature/);
  assert.equal(await f.inspect(f.destination), 'old');
});

test('refuses source/destination aliases and preserves conflicting commands', async t => {
  const f = await fixture(t);
  await assert.rejects(installBundle({ ...f.options, destination: f.source }), /different locations/);
  const alias = join(f.root, 'alias'); await symlink(join(f.root, 'build'), alias);
  await assert.rejects(installBundle({ ...f.options, destination: join(alias, 'Melly.app') }), /different locations/);
  const bin = join(f.root, 'bin'); await mkdir(bin); await writeFile(join(bin, 'melly'), 'another command');
  await assert.rejects(linkLauncher(f.destination, bin), /another command/);
  assert.equal(await readFile(join(bin, 'melly'), 'utf8'), 'another command');
});

test('launcher remains stable across replacement', async t => {
  const f = await fixture(t); const bin = join(f.root, 'bin');
  const command = await linkLauncher(f.destination, bin);
  await installBundle(f.options);
  assert.equal(await readFile(command, 'utf8'), 'new');
  assert.equal(await linkLauncher(f.destination, bin), command);
});

test('cask metadata rejects unverified placeholders and injected versions', () => {
  const template = 'version "@VERSION@"\nsha256 "@SHA256@"\n';
  assert.throws(() => renderCask(template, '1.0.0', 'todo'), /SHA-256/);
  assert.throws(() => renderCask(template, '1.0.0"\nraise "bad', 'a'.repeat(64)), /semantic version/);
  assert.equal(renderCask(template, '1.0.1', 'a'.repeat(64)), `version "1.0.1"\nsha256 "${'a'.repeat(64)}"\n`);
});

test('build and installer resolve custom target directories from the checkout', () => {
  const root = fileURLToPath(new URL('../', import.meta.url));
  assert.equal(cargoTargetDir({}), resolve(root, 'target'));
  assert.equal(cargoTargetDir({ CARGO_TARGET_DIR: '../shared-target' }), resolve(root, '../shared-target'));
  assert.equal(cargoTargetDir({ CARGO_TARGET_DIR: '/tmp/melly-target-fixture' }), '/tmp/melly-target-fixture');
});
