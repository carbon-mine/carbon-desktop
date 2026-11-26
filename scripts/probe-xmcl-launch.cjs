// SPDX-License-Identifier: MPL-2.0
// Optional package comparison. An isolated fixture supplies the package and pinned metadata.
const assert = require('node:assert/strict')
const { createHash } = require('node:crypto')
const { readFileSync } = require('node:fs')
const { delimiter, join, resolve } = require('node:path')
const { performance } = require('node:perf_hooks')

const [inputs, packageRoot, pinsFile] = process.argv.slice(2)
assert.ok(inputs && packageRoot && pinsFile, 'Supply the metadata, package, and pin paths')
const { Version, generateArguments } = require(resolve(packageRoot))
const pins = JSON.parse(readFileSync(pinsFile))

async function probe(pin, name) {
  const raw = readFileSync(join(inputs, `${pin.version}.json`))
  assert.equal(createHash('sha256').update(raw).digest('hex'), pin.sha256)
  const resourcePath = '/Synthetic Review/content'
  const platform = { name, arch: 'x64', version: '10.0.22631' }
  const gamePath = '/Synthetic Review/game'
  const version = Version.resolve(resourcePath, [Version.normalizeVersionJson(raw.toString(), resourcePath, platform)])
  const launch = () => generateArguments({
    version,
    platform,
    resourcePath,
    gamePath,
    javaPath: '/Synthetic Review/managed/bin/java',
    nativeRoot: '/Synthetic Review/scratch',
    gameIcon: 'fixture',
    gameProfile: { id: 'b031a91d0dd04d14812b333102819a66', name: 'SyntheticPlayer' },
    accessToken: 'fixture-reference',
    launcherName: 'Carbon Desktop',
    launcherBrand: '0.0.0',
    features: { has_custom_resolution: { resolution_width: 1280, resolution_height: 720 } },
    maxMemory: 2048,
    extraJVMArgs: [],
  })
  let start = performance.now()
  const args = await launch()
  const initial = performance.now() - start
  start = performance.now()
  for (let attempt = 0; attempt < 50; attempt++) {
    assert.deepEqual(await launch(), args)
  }
  const average = (performance.now() - start) / 50
  const classpath = args[args.indexOf('-cp') + 1].split(delimiter)
  const expected = name === 'windows' ? pin.windowsClasspath : pin.linuxClasspath
  const main = args.indexOf('net.minecraft.client.main.Main')
  const unresolved = args.filter(arg => arg.includes('${'))
  assert.ok(main > 1 && args.includes('fixture-reference'))
  assert.equal(classpath.length, expected)
  assert.deepEqual(unresolved.sort(), ['${auth_xuid}', '${clientid}'])
  console.log(`${pin.version} ${name} rules: ${classpath.length}/${expected} classpath, ${main - 1} JVM args, ${args.length - main - 1} game args, ${unresolved.length} unresolved placeholders, ${initial.toFixed(3)} ms first, ${average.toFixed(3)} ms warmed mean`)
}

async function main() {
  for (const pin of pins) {
    for (const platform of ['windows', 'linux']) {
      await probe(pin, platform)
    }
  }
  console.log(`Node process peak RSS: ${process.resourceUsage().maxRSS} kB`)
}

main().catch(error => {
  console.error(error.message)
  process.exitCode = 1
})
