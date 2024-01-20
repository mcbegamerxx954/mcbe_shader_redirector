#info
This crate is made to redirect MCBE shaders to ones from resource packs externally so that it can work with any mcbe version and even multiple platforms.
for now it only supports android

the tutorial is not 100% guaranteed to work yet and the APK might crash on some phones.

#Tutorial on how to use:
step 1: get the .so for your arch and rename it to "libmcbe_r.so"
step 2: place it in the mcbe APK at this path: "libs/<arch>/"
step 3: edit classes.dex in mcbe and go to the com.mojang.minecraftpe.MainActivity class and search function OnCreate and paste this inside of it :
```
const-string v0, "mcbe_r"
invoke-static {v0}, Ljava/lang/System;->loadLibrary(Ljava/lang/String;)V
```
step 4: now if you did everything correctly you should have a patched mcbe that redirects shaders.