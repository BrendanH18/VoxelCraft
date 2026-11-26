#!/usr/bin/env bash
# run-tag.sh TAG: one tag's save fixture in a fresh Boat sandbox (voxelcraft-saves-<tag>, made
# from voxelcraft-warm, with this folder copied to ~/bin): installs Xvfb and Mesa's software
# Vulkan, clones the repository, builds the tag and runs make-fixture.sh. The result is
# ~/out/TAG.tar.gz (unpack it and run collect.py on the folder); ~/status says how far it got.
set -uo pipefail
tag=$1
echo running >~/status
{
  sudo apt-get update -qq --allow-releaseinfo-change
  sudo DEBIAN_FRONTEND=noninteractive NEEDRESTART_SUSPEND=1 apt-get install -y -qq xvfb xauth mesa-vulkan-drivers libvulkan1 libx11-6 libxcursor1 libxrandr2 libxi6 libxkbcommon-x11-0 xdotool >/dev/null
} >~/apt.log 2>&1
[ -d ~/vc ] || git clone -q https://github.com/BrendanH18/VoxelCraft.git ~/vc
if ! ~/bin/build-tag.sh "$tag" >~/build.log 2>&1; then echo "build-failed" >~/status; exit 1; fi
~/bin/make-fixture.sh "$tag" 91 4242 >~/fx.log 2>&1
cp ~/build.log ~/fx.log ~/out/$tag/ 2>/dev/null
tar -C ~/out -czf ~/out/$tag.tar.gz --exclude=data --exclude=reopen "$tag"
echo done >~/status
