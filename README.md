# NOTICE
This does not work properly and i dont know if it ever will
because minecraft precaches worlds (yeah)

So i will not polish this till i find a way to fix that

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
step 1: get the .so for your arch and rename it to "libmcbe_r.so"

step 2: place it in the mcbe APK at this path: "libs/(arch)/"

step 3: edit classes.dex in mcbe and go to the com.mojang.minecraftpe.MainActivity class and search function OnCreate and paste this inside of it :
```
const-string v0, "mcbe_r"
invoke-static {v0}, Ljava/lang/System;->loadLibrary(Ljava/lang/String;)V
```

step 4: now if you did everything correctly you should have a patched mcbe that redirects shaders.
