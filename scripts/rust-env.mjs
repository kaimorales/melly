import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const projectRoot = fileURLToPath(new URL('../', import.meta.url));
export function cargoTargetDir(env = process.env) {
  return resolve(projectRoot, env.CARGO_TARGET_DIR || 'target');
}
export function rustEnv() {
  const candidates=[join(homedir(),'.cargo','bin'),join(homedir(),'.rustup','toolchains','stable-aarch64-apple-darwin','bin'),join(homedir(),'.rustup','toolchains','stable-x86_64-apple-darwin','bin')];
  const bins=candidates.filter(p=>existsSync(join(p,'cargo')));
  return {...process.env,CARGO_TARGET_DIR:cargoTargetDir(),PATH:[...bins,process.env.PATH].join(':')};
}
