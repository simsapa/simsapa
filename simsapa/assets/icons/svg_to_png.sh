#!/bin/bash

for i in ./svg/*.svg; do
    echo $i
    inkscape --export-type=png -w 32 -h 32 $i
done

mv ./svg/*.png ./32x32
