# Table OCR

A simple table OCR approach in rust, preserving structure, with a manually adjustable table/grid stencil.

Extraction quality will mostly depend on chosen OCR backend, which processes each entry independently and in parallel. 
This backed can be changed.

![Image](./example.png)

Example input from [techrepublic](https://www.techrepublic.com/article/tiobe-index-language-rankings/).

TODO: 

 - Automatic initial grid (row/column detection).
 - Non (ui) blocking extraction.

