# MCBER
This crate is made to redirect MCBE shaders to ones from resource packs externally so that it can work with any mcbe version and even multiple platforms.
for now it only supports **Android** and also **ChromeOS**.

> [!NOTE]
> This repo is a dependency of [draco-injector](https://github.com/mcbegamerxx954/draco-injector).<br>

> [!WARNING]
> The redirector is still unstable and might have some bugs or crashes, please [report those in this repo](https://github.com/mcbegamerxx954/mcbe_shader_redirector/issues).

#### Confirmed working with
+ 1.20.73
+ 1.20.81
+ 1.21.0.03

>[!TIP]
> This can automatically update renderdragon shaders to work using code ported to rust of [MaterialBinTool](https://github.com/ddf8196/MaterialBinTool) made by [ddf8196](https://github.com/ddf8196).<br>*formatVersion V1 to V2 to be specific*

## Usage
1. Go to [Releases](https://github.com/mcbegamerxx954/mcbe_shader_redirector/releases/latest) and download the `.so` file for your arch and rename it to `libmcbe_r.so`

2. Open up your minecraft.apk. Place `libmcbe_r.so` in **/lib/arch**

3. Make the library start with mc by any of these methods:

> ### Method 1 (Draco Injector) (Recommended)
> **This method automatically creates patched APK with MCBER.**
> + Download [Draco Injector](https://github.com/mcbegamerxx954/draco-injector/releases/tag/v0.1.7) for your platform and follow [instructions](https://github.com/Sparklight77/DroidDraco).
> + Or use automated GUI script like [DroidDraco](https://github.com/Sparklight77/DroidDraco) or [MineDraco](https://github.com/CallMeSoumya2063/MineDraco).

<br>

> ### Method 2 (Dex)
> + Open/Extract classes.dex in minecraft APK
> + Search the com.mojang.minecraftpe.MainActivity class 
> + Inside of it, search the function OnCreate and paste this inside of it:
> ```smali
> const-string v0, "mcbe_r"
> invoke-static {v0}, Ljava/lang/System;->loadLibrary(Ljava/lang/String;)V
> invoke-virtual {p0}, Lcom/mojang/minecraftpe/MainActivity;->dracoSetupStorage()V
> ```
> + Then just before the function add this: 
> ```smali
> .method public native dracoSetupStorage()V
> .end method
> ```
> ![image](https://github.com/mcbegamerxx954/mcbe_shader_redirector/assets/154642722/4549bdcf-75f3-4a3a-9afc-d7c3246a20ee)
<br>
> ![image](https://github.com/mcbegamerxx954/mcbe_shader_redirector/assets/40156662/5b9ab661-c54f-4982-9baf-4ad4b3006a4b)<br>
> <sup><sub>Done using [MT Manager](https://mt2.cn/download/)</sub></sup>

<br>

> ### Method 3 (Patchelf)
> + Download [patchelf](https://github.com/NixOS/patchelf/releases/latest) binary
> + Extract the libminecraftpe.so library from **lib/(arch)**
> + Run patchelf on it to make libmcbe_r a needed library:
> (replace path/to/ with the path to the library)
> ```bash
> patchelf path/to/libminecraftpe.so --add-needed libmcbe_r.so
> ```

4. Now if you did everything correctly you should have a patched mcbe that redirects shaders.

## How to build (PC)
+ Install rust using [rustup](https://rustup.rs/) if you dont have it 
+ Download the ndk
+ Add android target using "rustup target add"
+ Setup rust to use the ndk depending on your platform
+ cd to this repo and do "cargo build --release --target your-android-target"
+ Search for your .so in target folder
+ Now you have it

## How to build (Android)
+ Install [Termux](https://f-droid.org/en/packages/com.termux/) if you dont have it
+ Update packages using "pkg upg"
+ Install essential stuff for building using "apt install build-essential"
+ Install rust using "pkg install rust"
+ git clone this repo
+ do "cargo build --release" on where the repo is
+ Now you should have your lib in target folder in repo dir
