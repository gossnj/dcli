#!/usr/bin/env python3
"""Run the real Android JNI contract on an explicitly selected disposable emulator."""
import argparse
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library', type=Path, required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--sdk', type=Path, default=Path(os.environ.get('ANDROID_HOME', Path.home() / 'Library/Android/sdk')))
    parser.add_argument('--java-home', type=Path, default=Path(os.environ.get('JAVA_HOME', '/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home')))
    parser.add_argument('--build-tools', default='36.0.0')
    args = parser.parse_args()
    library = args.library.resolve(strict=True)
    adb = [str(args.sdk / 'platform-tools/adb'), '-s', args.serial]
    remote = '/data/local/tmp/dcli-jni-' + uuid.uuid4().hex
    source = Path(__file__).parent / 'android-jni/DcliStore.java'
    env = dict(os.environ, JAVA_HOME=str(args.java_home))
    with tempfile.TemporaryDirectory(prefix='dcli-jni-') as temporary:
        root = Path(temporary)
        classes, dex = root / 'classes', root / 'dex'
        classes.mkdir()
        dex.mkdir()
        android_jar = args.sdk / 'platforms/android-36/android.jar'
        subprocess.run([str(args.java_home / 'bin/javac'), '--release', '11', '-classpath', str(android_jar),
                        '-d', str(classes), str(source)], check=True, env=env)
        subprocess.run([str(args.sdk / 'build-tools' / args.build_tools / 'd8'), '--min-api', '26',
                        '--lib', str(android_jar), '--output', str(dex),
                        *map(str, sorted(classes.rglob('*.class')))], check=True, env=env)
        subprocess.run([*adb, 'shell', 'mkdir', remote], check=True)
        try:
            subprocess.run([*adb, 'push', str(dex / 'classes.dex'), remote + '/classes.dex'], check=True)
            subprocess.run([*adb, 'push', str(library), remote + '/libdcli_ffi.so'], check=True)
            command = 'CLASSPATH=' + shlex.quote(remote + '/classes.dex') + ' ' + shlex.join([
                'app_process', '/system/bin', 'com.ottercreeksoftware.lastbanner.data.dcli.DcliStore',
                remote + '/libdcli_ffi.so', remote + '/cases'])
            result = subprocess.run([*adb, 'shell', command], timeout=90)
            return result.returncode
        finally:
            subprocess.run([*adb, 'shell', 'rm', '-rf', remote], check=True)


if __name__ == '__main__':
    raise SystemExit(main())
