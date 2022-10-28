# Install on Linux

Download the latest `.AppImage.zip` from [Releases](https://github.com/simsapa/simsapa/releases/).

Extract and add executable permissions to the `.AppImage` file.

```
chmod +x *.AppImage
```

**Ubuntu 22.04:** The HTML content pages will be blank, you have to start Simsapa with the following env variable:

``` shell
QTWEBENGINE_DISABLE_SANDBOX=1 ./Simsapa_Dhamma_Reader-0.1.8a1-x86_64.AppImage
```

For the app launcher, it can be useful to create a `simsapa.desktop` file in `~/.local/share/applications` such as:

```
[Desktop Entry]
Encoding=UTF-8
Name=Simsapa
Terminal=false
Type=Application
Exec=env QTWEBENGINE_DISABLE_SANDBOX=1 /path/to/Simsapa_Dhamma_Reader-0.1.8a1-x86_64.AppImage
```


