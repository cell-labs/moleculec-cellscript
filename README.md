## molecule cellscript plugin

A plugin for the molecule serialization system to generate CellScript code.

### Use

```shell
$ cargo install moleculec moleculec-cellscript
$ moleculec --language cellscript --schema-file "your-schema-file" | gofmt > "your-cellscript-file"
```

### Test

```shell
./run.sh
```

## License

Licensed under [MIT License][MIT License].

[MIT License]: LICENSE
