#!/bin/bash

CURRENT_VERSION=$(grep current_version .bumpversion.cfg | head -n 1 | cut -d' ' -f 3)
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
WIKI=$SCRIPT_DIR/../blobtk.wiki

update_block () {
    cd rust
    MARKER=$1
    shift
    FILE=$1
    FILE_PATH=$WIKI/$FILE
    shift
    awk -v FLAGS="$@" '/^```'"$MARKER"'/{del=1;print;system("cargo run -- "FLAGS);next} {if(!del)print} /^```/{if(del){del=0;print}}' $FILE_PATH > $FILE_PATH.tmp
    cd -
}

update_block "sh help text" blobtk.md "-h"
update_block "sh help text" blobtk-depth.md "depth -h"
update_block "sh help text" blobtk-filter.md "filter -h"
update_block "sh help text" blobtk-plot.md "plot -h"
update_block "sh help text" blobtk-taxonomy.md "taxonomy -h"

sed -E "s:download/[[:digit:]]+\.[[:digit:]]+\.[[:digit:]]+/blobtk-linux:download/v$CURRENT_VERSION/blobtk-linux:; \
     s:download/[[:digit:]]+\.[[:digit:]]+\.[[:digit:]]+/blobtk-macos-x64:download/v$CURRENT_VERSION/blobtk-macos-x64:; \
     s:download/[[:digit:]]+\.[[:digit:]]+\.[[:digit:]]+/blobtk-macos-amd64:download/v$CURRENT_VERSION/blobtk-macos-amd64:" \
     $WIKI/Home.md > $WIKI/Home.md.tmp

