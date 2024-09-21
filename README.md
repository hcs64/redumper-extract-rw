# redumper-extract-rw

Deinterleave and correct R-W subchannel packs from a redumper .subcode dump

## Usage

```
redumper-extract-rw image_name out.cdg
```

will parse `image_name.subcode` and `image_name.toc` and create `out.cdg`. The .toc is needed to know when to stop before lead-out, if it is missing then all sectors (past 10:02 for lead-in) will be used.

## Changelog

* 0.4
  * report correction count by pack mode
  * delay by 2 sectors to match cdrdao Cooked RW output
  * check for non-zero packs outside of TOC range
  * use .toc to skip leadout
* 0.3 - 2024-02-17
  * switch P and Q naming, don't attempt Q correction
* 0.2 - 2024-02-16
  * implement P parity error correction
* 0.1 - 2024-02-07
  * drop packs with bad P parity
* 0.0 - 2024-02-07
  * gather R-W subchannels
