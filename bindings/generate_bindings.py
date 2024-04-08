#!/usr/bin/env python3
import argparse
import subprocess
from pathlib import Path

def uniffi_gen(executable: str, *args):
    print(f'Running {executable} {" ".join(map(str, args))}')
    subprocess.run([executable, *map(str, args)], check=True)

def uniffi_gen_go(udl: Path, output_dir: Path):
    uniffi_gen("uniffi-bindgen-go", udl, "-o", output_dir)


def uniffi_gen_cs(udl: Path, output_dir: Path, config_file: Path):
    uniffi_gen("uniffi-bindgen-cs", udl, "-o", output_dir, "-c", config_file)


def uniffi_gen_swift(udl: Path, output_dir: Path):
    uniffi_gen("uniffi-bindgen", "generate", udl, "--language", "swift", "-o", output_dir)


def uniffi_gen_kotlin(udl: Path, output_dir: Path):
    uniffi_gen("uniffi-bindgen", "generate", udl, "--language", "kotlin", "-o", output_dir)

def uniffi_gen_python(udl: Path, output_dir: Path):
    uniffi_gen("uniffi-bindgen", "generate", udl, "--language", "python", "-o", output_dir)

def uniffi_gen_cpp(udl: Path, output_dir: Path):
    uniffi_gen("uniffi-bindgen-cpp", udl, "-o", output_dir)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description="Generate UniFFI bindings from the UDL file", exit_on_error=True)
    parser.add_argument("-d", "--directory", help="Output directory", required=True)
    parser.add_argument("-u", "--udl", help="UDL file", required=True)
    parser.add_argument("-c", "--config", help="Uniffi config file")
    parser.add_argument("-a", "--all", help="Generate all bindings", action='store_true')
    parser.add_argument("--go", help="Generate go bindings", action='store_true')
    parser.add_argument("--csharp", help="Generate charp bindings", action='store_true')
    parser.add_argument("--swift", help="Generate swift bindings", action='store_true')
    parser.add_argument("--kotlin", help="Generate kotlin bindings", action='store_true')
    parser.add_argument("--python", help="Generate python bindings", action='store_true')
    parser.add_argument("--cpp", help="Generate cpp bindings", action='store_true')
    args = parser.parse_args()

    output_dir = Path(args.directory)
    udl_file = Path(args.udl)

    if args.all:
        config_file = Path(args.config)
        uniffi_gen_go(udl_file, output_dir.joinpath(f"go"))
        uniffi_gen_cs(udl_file, output_dir.joinpath(f"csharp"), config_file)
        uniffi_gen_kotlin(udl_file, output_dir.joinpath(f"kotlin"))
        uniffi_gen_swift(udl_file, output_dir.joinpath(f"swift"))
        uniffi_gen_python(udl_file, output_dir.joinpath(f"python"))
        uniffi_gen_cpp(udl_file, output_dir.joinpath(f"cpp"))
    else:
        if args.go:
            uniffi_gen_go(udl_file, output_dir.joinpath(f"go"))
        if args.csharp:
            config_file = Path(args.config)
            uniffi_gen_cs(udl_file, output_dir.joinpath(f"csharp"), config_file)
        if args.swift:
            uniffi_gen_swift(udl_file, output_dir.joinpath(f"swift"))
        if args.kotlin:
            uniffi_gen_kotlin(udl_file, output_dir.joinpath(f"kotlin"))
        if args.python:
            uniffi_gen_python(udl_file, output_dir.joinpath(f"python"))
        if args.cpp:
            uniffi_gen_cpp(udl_file, output_dir.joinpath(f"cpp"))