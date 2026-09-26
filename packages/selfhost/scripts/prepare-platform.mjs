#!/usr/bin/env node
import { createHash } from 'node:crypto';
import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  writeFileSync,
} from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const source = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repository = resolve(source, '../..');
const args = process.argv.slice(2);
const values = {};
for (let index = 0; index < args.length; index += 2) {
  if (
    !['--binary', '--output', '--platform', '--arch'].includes(args[index]) ||
    !args[index + 1] ||
    Object.hasOwn(values, args[index])
  )
    throw new Error(
      'Use --binary FILE --output NEW_DIRECTORY [--platform win32|linux|darwin] [--arch x64|arm64].',
    );
  values[args[index]] = args[index + 1];
}
if (!values['--binary'] || !values['--output'])
  throw new Error('--binary and --output are required.');
const platform = values['--platform'] || process.platform,
  arch = values['--arch'] || process.arch;
if (!['win32', 'linux', 'darwin'].includes(platform) || !['x64', 'arm64'].includes(arch))
  throw new Error('Unsupported target platform or architecture.');
if (platform !== process.platform)
  throw new Error(
    'Stage on the target operating system so npm preserves its native executable permissions.',
  );
const binary = realpathSync(values['--binary']),
  output = resolve(values['--output']);
let ancestor = output;
while (!existsSync(ancestor)) {
  const parent = dirname(ancestor);
  if (parent === ancestor)
    throw new Error('The staging directory has no accessible filesystem root.');
  ancestor = parent;
}
const physicalOutput = resolve(realpathSync(ancestor), relative(ancestor, output));
const within = relative(realpathSync(repository), physicalOutput);
const outside =
  within === '..' || within.startsWith('../') || within.startsWith('..\\') || isAbsolute(within);
if (within && !outside && !within.replaceAll('\\', '/').startsWith('.local/'))
  throw new Error('Stage packages outside the repository or inside its ignored .local directory.');
if (physicalOutput === realpathSync(repository) || existsSync(output))
  throw new Error('Output must be a new staging directory.');
const bytes = readFileSync(binary);
const magic = bytes.subarray(0, 4).toString('hex');
if (
  platform === 'win32'
    ? !magic.startsWith('4d5a')
    : platform === 'linux'
      ? magic !== '7f454c46'
      : !['feedface', 'feedfacf', 'cefaedfe', 'cffaedfe', 'cafebabe', 'bebafeca'].includes(magic)
)
  throw new Error('Executable header does not match the selected platform.');
let detectedArch;
if (platform === 'win32') {
  const offset = bytes.length >= 64 ? bytes.readUInt32LE(60) : bytes.length;
  if (
    offset + 6 <= bytes.length &&
    bytes.subarray(offset, offset + 4).toString('hex') === '50450000'
  )
    detectedArch = { 0x8664: 'x64', 0xaa64: 'arm64' }[bytes.readUInt16LE(offset + 4)];
} else if (platform === 'linux' && bytes.length >= 20 && bytes[4] === 2 && bytes[5] === 1) {
  detectedArch = { 62: 'x64', 183: 'arm64' }[bytes.readUInt16LE(18)];
} else if (platform === 'darwin' && bytes.length >= 8 && ['feedfacf', 'cffaedfe'].includes(magic)) {
  const machine = magic === 'cffaedfe' ? bytes.readUInt32LE(4) : bytes.readUInt32BE(4);
  detectedArch = { 0x01000007: 'x64', 0x0100000c: 'arm64' }[machine];
}
if (detectedArch !== arch)
  throw new Error(
    'Executable architecture does not match the selected target. Use a single-architecture x64 or arm64 build.',
  );
const host = `${platform}-${arch}`,
  filename = `selfhost-${host}${platform === 'win32' ? '.exe' : ''}`;
mkdirSync(join(output, 'native'), { recursive: true });
cpSync(join(source, 'bin'), join(output, 'bin'), { recursive: true });
copyFileSync(join(source, 'README.md'), join(output, 'README.md'));
const metadata = JSON.parse(readFileSync(join(source, 'package.json'), 'utf8'));
metadata.os = [platform];
metadata.cpu = [arch];
delete metadata.scripts;
delete metadata.private;
writeFileSync(join(output, 'package.json'), JSON.stringify(metadata, null, 2) + '\n');
copyFileSync(join(repository, 'LICENSE'), join(output, 'LICENSE'));
copyFileSync(binary, join(output, 'native', filename));
chmodSync(join(output, 'native', filename), 0o755);
chmodSync(join(output, 'bin', 'selfhost.mjs'), 0o755);
writeFileSync(
  join(output, 'native', 'manifest.json'),
  JSON.stringify(
    {
      schema: 1,
      binaries: {
        [host]: { file: filename, sha256: createHash('sha256').update(bytes).digest('hex') },
      },
    },
    null,
    2,
  ) + '\n',
);
console.log(
  `Prepared ${host} package. Review it and run npm pack in the staging directory. Nothing was published.`,
);
