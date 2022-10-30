# Simsapa Dhamma Reader

A sutta reader and Pali dictionary application.

Short feature demo: <https://vimeo.com/677880749>

![Sutta Search](docs/images/sutta-search-screenshot.jpg)

![Sutta Links](docs/images/sutta-links-screenshot.jpg)

![Sutta Study](docs/images/sutta-study-screenshot.png)

## Install

Download the latest build from [Releases](https://github.com/simsapa/simsapa/releases/) for Linux (.AppImage), Mac (.dmg) or Windows (.msi).

On the first run, it downloads the application database and fulltext index.

- [Install on Linux](docs/install-linux.md)
- [Install on MacOS](docs/install-macos.md)
- [Install on Windows](docs/install-windows.md)
- [Development: Running from Source](docs/development-running-from-source.md)

## Removing the Application Database

The Simsapa application database (where the suttas, dictionaries, etc. are stored) is not removed when un-installing Simsapa.

Use the terminal to remove the applications local data folder:

**Linux:**

``` shell
rm -r ~/.local/share/simsapa
```

**MacOS:**

``` shell
rm -r ~/Library/Application\ Support/simsapa
```

**Windows:**

``` shell
rmdir /s /q C:\Users\%USERNAME%\AppData\Local\simsapa
```


