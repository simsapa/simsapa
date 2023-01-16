
if exist build\ rmdir /S /Q build
if exist dist\ rmdir /S /Q dist

pyinstaller run.py ^
    --name "Simsapa Dhamma Reader" ^
    --onefile ^
    -w ^
    --clean ^
    --noupx ^
    -i "simsapa\assets\icons\appicons\simsapa.ico" ^
    --add-data "simsapa\assets;simsapa\assets" ^
    --add-data "simsapa\alembic;simsapa\alembic" ^
    --add-data "simsapa\alembic.ini;simsapa\alembic.ini"

:: Ensure blank line after cmd with caret
echo ""
