import { spawn } from 'node:child_process';
import { rustEnv } from './rust-env.mjs';
const child=spawn('cargo',process.argv.slice(2),{env:rustEnv(),stdio:'inherit'});
child.on('error',()=>{console.error('Rust is not installed. Install Rust, then run this command again.');process.exitCode=1});
child.on('exit',code=>{process.exitCode=code??1});
