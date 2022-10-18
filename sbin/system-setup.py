#!/usr/bin/env python3

import sys
import os
import argparse

HERE = os.path.abspath(os.path.dirname(__file__))
ROOT = os.path.abspath(os.path.join(HERE, ".."))
READIES = os.path.join(ROOT, "deps/readies")
sys.path.insert(0, READIES)
import paella

#----------------------------------------------------------------------------------------------

class RedisJSONSetup(paella.Setup):
    def __init__(self, args):
        paella.Setup.__init__(self, args.nop)

    def common_first(self):
        self.install_downloaders()
        self.run("%s/bin/enable-utf8" % READIES, sudo=self.os != 'macos')
        self.install("git unzip rsync")

        if self.osnick == 'ol8':
            self.install("tar")
        self.run("%s/bin/getclang --modern" % READIES)
        if not self.has_command("rustc"):
            self.run("%s/bin/getrust" % READIES)
        self.run("%s/bin/getcmake --usr" % READIES)

    def debian_compat(self):
        self.install("python3-dev")
        self.run("%s/bin/getgcc" % READIES)

    def redhat_compat(self):
        self.install("redhat-lsb-core")
        self.run("%s/bin/getgcc --modern" % READIES)

        if not self.platform.is_arm():
            self.install_linux_gnu_tar()

    def fedora(self):
        self.run("%s/bin/getgcc" % READIES)

    def macos(self):
        self.install_gnu_utils()
        self.install("binutils")
        # self.run("%s/bin/getgcc" % READIES)
        self.run("%s/bin/getclang --modern" % READIES)

    def common_last(self):
        self.run("{PYTHON} {READIES}/bin/getrmpytools --modern --rltest-version master".format(PYTHON=self.python, READIES=READIES))
        self.pip_install("-r %s/tests/pytest/requirements.txt" % ROOT)
        self.run("%s/bin/getaws" % READIES)
        self.run("NO_PY2=1 %s/bin/getpudb" % READIES)

#----------------------------------------------------------------------------------------------

parser = argparse.ArgumentParser(description='Set up system for build.')
parser.add_argument('-n', '--nop', action="store_true", help='no operation')
args = parser.parse_args()

RedisJSONSetup(args).setup()
