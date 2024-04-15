# MCBER
This crate is made to redirect MCBE shaders to ones from resource packs externally so that it can work with any mcbe version and even multiple platforms.
for now it only supports android

the tutorial is not 100% guaranteed to work yet and the APK might crash on some phones.

## How to build(pc):
+ Install rust using [rustup](https://rustup.rs/) if you dont have it 
+ Download the ndk
+ Add android target using "rustup target add"
+ Setup rust to use the ndk depending on your platform
+ cd to this repo and do "cargo build --release --target your-android-target"
+ Search for your .so in target folder
+ Now you have it

## How to build(android)
+ Install termux if you dont have it
+ Update packages using "pkg upg"
+ Install essential stuff for building using "pkg install build-essential"
+ Install rust using "pkg install rust"
+ git clone this repo
+ do "cargo build --release" on where the repo is
+ Now you should have your lib in target folder in repo dir

## Tutorial on how to use:
step 1: Go to releases and open the latest one

step 2: Download the .so for your arch and rename it to "libmcbe_r.so"

step 2: Place it in the mc APK at this path: "libs/(arch)/"

step 3: Make the library start with mc by any of these methods:

Method 1 (Dex):
+ Open/Extract classes.dex in minecraft APK
+ Search the com.mojang.minecraftpe.MainActivity class 
+ Inside of it, search the function OnCreate and paste this inside of it:
```
const-string v0, "mcbe_r"
invoke-static {v0}, Ljava/lang/System;->loadLibrary(Ljava/lang/String;)V
```

Method 2 (Patchelf, untested):
+ Install patchelf
+ Extract the libminecraftpe.so library in "libs/(arch)"
+ Run patchelf on it to make libmcbe_r a needed library:
(replace path/to/ with the path to the library)
```
patchelf path/to/libminecraftpe.so --add-needed libmcbe_r.so
```

step 4: now if you did everything correctly you should have a patched mcbe that redirects shaders.
