# redumper-extract-rw

Deinterleave and correct R-W subchannel packs from a redumper .subcode dump

## Usage

```
redumper-extract-rw image_name out.cdg
```

This will parse `image_name.subcode` and `image_name.toc` and create `out.cdg`.

The .toc is used to know when to stop before lead-out, if it is missing then all sectors (past 10:02 for lead-in overread) will be used.

### .sub output

Alternate usage is:

```
redumper-extract-rw image_name out.cdg out.sub
```

This will also produce a bitpacked `out.sub` file, not corrected or deinterleaved.


## Changelog

* 0.4
  * report correction count by pack mode
  * delay by 2 sectors to match Cooked RW output
  * check for non-zero packs outside of TOC range
  * use .toc to skip leadout
  * incorporate .sub output from redumper-sub
* 0.3 - 2024-02-17
  * switch P and Q naming, don't attempt Q correction
* 0.2 - 2024-02-16
  * implement P parity error correction
* 0.1 - 2024-02-07
  * drop packs with unexpected P (actually Q) parity
* 0.0 - 2024-02-07
  * gather R-W subchannels
