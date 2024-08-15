use molecule_codegen::IntermediateFormat;
use moleculec_cellscript::build_commandline;

fn main() {
    build_commandline(IntermediateFormat::JSON).execute();
}
