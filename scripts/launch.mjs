import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { builtApp, installedApp, inspectBundle } from './install-local.mjs';

try {
  const app = existsSync(installedApp) ? installedApp : builtApp;
  await inspectBundle(app);
  execFileSync('/usr/bin/open', [app]);
} catch (error) { console.error(error.message); process.exitCode = 1; }
