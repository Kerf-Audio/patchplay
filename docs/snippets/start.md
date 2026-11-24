# Start

Every other snippet cables to a module called `clock` and a module called `out`. Paste this first, or name your own clock and output the same.

### Clock and output

The clock and the master output that every other snippet cables to. Paste this one first.

```kerf snippet
# Clock and output
# The clock and the master output that every other snippet cables to. Paste
#   this one first.
# Port: clock.clock (clock): one pulse per 16th note. clock.div4 is one per
#   beat.
# Port: out.l, out.r (audio): the speakers. Every sound snippet already cables
#   to these.

part master "Master" colour #8a8f98
  module clock Tally
  module out Finish volume=0.7 glue=0.3 ceiling=0.8
```
