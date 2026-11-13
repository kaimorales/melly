import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
export function rustEnv() {
  const candidates=[join(homedir(),'.cargo','bin'),join(homedir(),'.rustup','toolchains','stable-aarch64-apple-darwin','bin'),join(homedir(),'.rustup','toolchains','stable-x86_64-apple-darwin','bin')];
  const bins=candidates.filter(p=>existsSync(join(p,'cargo')));
  return {...process.env,PATH:[...bins,process.env.PATH].join(':')};
}
