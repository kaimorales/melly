import { spawn } from 'node:child_process';
import { rustEnv } from './rust-env.mjs';
const child=spawn(process.execPath,['node_modules/@tauri-apps/cli/tauri.js',...process.argv.slice(2)],{env:rustEnv(),stdio:'inherit'});
child.on('error',e=>{console.error(e.message);process.exitCode=1});
child.on('exit',code=>{process.exitCode=code??1});
