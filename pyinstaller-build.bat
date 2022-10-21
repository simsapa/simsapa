
if exist build\ rmdir /S /Q build
if exist dist\ rmdir /S /Q dist

pyinstaller run.py ^
    --name simsapa ^
    --onefile ^
    -w ^
    --add-data "simsapa\assets;simsapa\assets" ^
    --add-data "simsapa\alembic;simsapa\alembic" ^
    --add-data "simsapa\alembic.ini;simsapa\alembic.ini"

:: Ensure blank line after cmd with caret
echo ""
