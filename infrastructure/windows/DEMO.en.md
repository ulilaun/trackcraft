# TrackCraft demo for Windows

[Deutsch](DEMO.md) · English

Made-up training data on Mallorca (almost two years of running, cycling and hiking, segments, routes, plan). For trying TrackCraft out. Nothing is installed, and no administrator rights are needed.

## Start

1. Unzip the ZIP, e.g. into the Downloads folder.
2. Double-click `demo.cmd`. A console window opens, and shortly afterwards the browser opens <https://localhost:8443>.
3. **Confirm the certificate warning.** On first access the browser warns (“Your connection is not private”, “Warning: Potential Security Risk Ahead”). That is expected for the demo: it created the certificate itself on first start, and no device knows it. Continue via **Advanced** and **Proceed to localhost (unsafe)** or **Accept the Risk and Continue**. Confirm this only for this demo address.

On first start Windows may warn “Windows protected your PC” because the program comes from the internet and is not signed. Then start it via **More info** and **Run anyway**.

Stop: close the console window or press Ctrl+C in it.

The interface starts in the language of your browser; switch it under Settings → Appearance.

## What the demo shows

On start, the data is shifted so that the last activity was yesterday. Dashboard, week, form and plan therefore always look current. A banner at the top shows that it is a demo. Importing your own files, creating segments and planning work as in a real installation.

## Folder, starting over, removing

- Everything is stored in `%LOCALAPPDATA%\TrackCraft-Demo` (database, original files, logs, certificate).
- Start over with the original demo data: `demo.cmd --reset`.
- Port 8443 in use: `demo.cmd --port 9443`.
- Remove: close the console window, delete the folder `%LOCALAPPDATA%\TrackCraft-Demo` and the unzipped folder.

An existing TrackCraft installation (service, `C:\ProgramData\TrackCraft`) is not affected. The demo uses its own folders and port and only shifts data that is marked as a demo database.

For a real installation there is the ZIP `trackcraft-<version>-windows-x86_64.zip` with `setup.cmd`.
