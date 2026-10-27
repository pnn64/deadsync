#!/usr/bin/env python3
"""Cargo rustc wrapper: native-crate feedback, unchanged sanitizer checks.

Only deadlib_image_png/deadlib_image retain cargo-fuzz's SanitizerCoverage
flags. Dependencies and harnesses retain ASan and all other compiler flags.
The runner audits object sections as well as these invocation records.
"""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

NATIVE = {'deadlib_image_png', 'deadlib_image'}


def without_feedback(arguments):
    passed, removed = [], []
    index = 0
    while index < len(arguments):
        argument = arguments[index]
        pair = argument == '-C'
        value = arguments[index + 1] if pair else argument[2:] if argument.startswith('-C') else ''
        feedback = value in {'passes=sancov', 'passes=sancov-module'}
        if value.startswith('passes=') and 'sancov' in value and not feedback:
            raise ValueError('Unexpected mixed SanitizerCoverage pass list: ' + value)
        if value.startswith('llvm-args=') and '-sanitizer-coverage-' in value:
            if not value.startswith('llvm-args=-sanitizer-coverage-') or len(value.split()) != 1:
                raise ValueError('Unexpected mixed SanitizerCoverage LLVM arguments: ' + value)
            feedback = True
        width = 2 if pair else 1
        (removed if feedback else passed).extend(arguments[index:index + width])
        index += width
    return passed, removed


def main():
    compiler, *arguments = sys.argv[1:]
    crate = arguments[arguments.index('--crate-name') + 1] if '--crate-name' in arguments else None
    passed, removed = (arguments, []) if crate in NATIVE or crate is None else without_feedback(arguments)
    if crate is not None:
        audit = Path(os.environ['DEADLIB_PNG_NATIVE_FUZZ_AUDIT'])
        audit.mkdir(parents=True, exist_ok=True)
        record = {'crate': crate, 'native_feedback': crate in NATIVE,
                  'compiler': compiler, 'original_arguments': arguments,
                  'passed_arguments': passed, 'removed_feedback_arguments': removed,
                  'wrapper_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
        (audit / f'{os.getpid()}.json').write_text(json.dumps(record, indent=2) + '\n')
    return subprocess.call([compiler, *passed])


if __name__ == '__main__':
    raise SystemExit(main())
