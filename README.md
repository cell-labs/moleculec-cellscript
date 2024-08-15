## molecule cellscript plugin

A plugin for the molecule serialization system to generate CellScript code.

### Use

```shell
$ cargo install moleculec moleculec-cellscript
$ moleculec --language cellscript --schema-file "your-schema-file" | gofmt > "your-cellscript-file"
```

### Testset

all [test](./test/testset/) from [molecule](https://github.com/nervosnetwork/molecule/tree/master/test)

you can run `make gen-test` to reproduce it.

## License

Licensed under [MIT License][MIT License].

[MIT License]: LICENSE
