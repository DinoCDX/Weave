# Test models

Complex models for visual tests are downloaded on demand by `scripts/fetch-test-models.sh` into `assets/external/`. That folder is git-ignored, so the models are not committed.

| file | source | size | notes |
|---|---|---|---|
| armadillo.obj | Stanford 3D Scanning Repository, mirrored by the common-3d-test-models collection (github.com/alecjacobson/common-3d-test-models) | about 50,000 vertices, 100,000 faces | The Stanford scans are for research use. The collection does not state a redistribution licence, so the models are not redistributed in this repository. Check the licence before any public release. |

Load a model in the showcase with `--model assets/external/armadillo.obj`.
