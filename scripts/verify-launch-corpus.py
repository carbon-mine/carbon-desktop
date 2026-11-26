#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Verify externally obtained launch corpus files without downloading or executing them."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
from zipfile import ZipFile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def check_hash(path, digest):
    with path.open("rb") as file:
        actual = hashlib.file_digest(file, "sha256").hexdigest()
    require(actual == digest, f"{path.name} has an unexpected SHA-256")


def check_vanilla(inputs, pins):
    for pin in pins:
        version = pin["version"]
        path = inputs / f"{version}.json"
        check_hash(path, pin["sha256"])
        data = json.loads(path.read_bytes())
        require(data["id"] == version, "Unexpected vanilla version")
        require(data["mainClass"] == "net.minecraft.client.main.Main", "Unexpected entry point")
        require(data["javaVersion"]["majorVersion"] == pin["javaMajor"], "Unexpected Java major")
        index = data["assetIndex"]
        require(index["id"] == pin["assetIndex"] and index["sha1"] == pin["assetIndexSha1"],
                "Unexpected asset index")
        print(f"{version}: modern manifest, Java {pin['javaMajor']}, asset index {index['id']}")


def check_comparison(inputs, pins):
    legacy = pins["legacy"]
    path = inputs / f"{legacy['id']}.json"
    check_hash(path, legacy["sha256"])
    data = json.loads(path.read_bytes())
    require(data["id"] == legacy["id"] and data["mainClass"] == legacy["mainClass"],
            "Unexpected legacy metadata")
    require(data["javaVersion"]["majorVersion"] == legacy["javaMajor"], "Unexpected legacy Java")
    require(data["assetIndex"]["id"] == legacy["assetIndex"], "Unexpected legacy asset index")
    require(isinstance(data.get("minecraftArguments"), str) == legacy["legacyArguments"],
            "Expected a legacy argument string")
    print(f"{legacy['id']}: legacy argument string and Java {legacy['javaMajor']}")

    forge = pins["forge"]
    path = inputs / f"forge-{forge['id']}-installer.jar"
    check_hash(path, forge["sha256"])
    with ZipFile(path) as jar:
        item = jar.getinfo(forge["entry"])
        require(item.file_size <= 1024 * 1024, "Forge profile exceeds the metadata size limit")
        data = json.loads(jar.read(item))
    require(data["minecraft"] == forge["minecraft"], "Unexpected Forge Minecraft version")
    processors = [proc["jar"] for proc in data["processors"]
                  if "sides" not in proc or "client" in proc["sides"]]
    require(processors == forge["clientProcessorArtifacts"], "Unexpected Forge client processors")
    print(f"Forge {forge['id']}: {len(processors)} client processors inspected as data")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", type=Path)
    args = parser.parse_args()
    corpus = Path(__file__).resolve().parents[1] / "fixtures" / "launch-corpus"
    try:
        check_vanilla(args.inputs, json.loads((corpus / "vanilla.json").read_bytes()))
        check_comparison(args.inputs, json.loads((corpus / "comparison.json").read_bytes()))
    except (KeyError, OSError, ValueError) as error:
        parser.exit(1, f"Corpus verification failed: {error}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
