import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { builtApp, inspectBundle } from './install-local.mjs';

const args = process.argv.slice(2);
if (args.some(arg => arg !== '--distribution') || args.length > 1) throw new Error('Use npm run verify:app [-- --distribution].');
const version = await inspectBundle(builtApp);
const binary = join(builtApp, 'Contents/MacOS/melly-desktop');
const printed = execFileSync(binary, ['--version'], { encoding: 'utf8', timeout: 10000 }).trim();
if (printed !== `Melly ${version}`) throw new Error('Bundle and executable versions differ.');
const help = execFileSync(binary, ['--help'], { encoding: 'utf8', timeout: 10000 });
if (!help.includes('--update') || !help.includes('--open')) throw new Error('App management commands are missing.');
if (args.includes('--distribution')) {
  const { spawnSync } = await import('node:child_process');
  const signature = spawnSync('/usr/bin/codesign', ['--display', '--verbose=4', builtApp], { encoding: 'utf8' });
  if (signature.status !== 0 || !/^Authority=Developer ID Application:/m.test(signature.stderr) || !/^TeamIdentifier=[A-Z0-9]{10}$/m.test(signature.stderr)) throw new Error('Distribution requires an Apple Developer ID Application signature.');
  execFileSync('/usr/bin/xcrun', ['stapler', 'validate', builtApp], { stdio: 'inherit' });
  execFileSync('/usr/sbin/spctl', ['--assess', '--type', 'execute', '--verbose', builtApp], { stdio: 'inherit' });
}
console.log(`Verified Melly ${version}: identity, signature and management commands${args.length ? ', Developer ID, notarization ticket and Gatekeeper' : ' (local build)'}.`);
