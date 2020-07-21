# lookr
Index and search command line utility.

## Overview
This was / is a lockdown project to implement a `locate` style service, in rust.

The daemon will run, indexing the paths in the configured location (and updating the index with any filesystem changes). The client will run, connect to the local daemon and query the index.

## Issues / TODO
The index does not do any permission checking - there is a plan to implement this but it is not done, so if the daemon is running as any given user, any other user can connect to it and see all paths that are indexed.

The plan to work around this is to generate a user-specific token on the filesystem, readable only by that user, that the client-user will need to provide in the query. This has the impact of: needing local fs access to get the token; validating that the user is who they say they are. The daemon will provide an endpoint to get the location of the tokens.

For the permissions, we will need to additionally build up an index of them and join it with the paths when queried (for rwx for the querying user).

The end result should be that the user should not be able to read paths where they don't have permission.

This is not an issue if running in a single-user environment.
