# Examples of Multi-Module Configurations
> For geospatial data processing, there are two routes to take for multi-module processing: either with or without including Module A. If included, Module A will always be listed first in the multi-module workflow, followed by Module P and then finally Module O. 

## Understanding the Entire Config
Multi-module configurations follow the same overall structure as single module configurations, simply with more than one module listed. Please reference the `README-single-modules.md` before attempting a multi-module approach.

The general structure of the configuration remains the same:

- Specify the input directory or series of directories:
    - There will still be the initial `input` value to include outside of the module definitions. This is generally going to be the directory (or specific subdirectories) of raw, level 1B imagery.
- Define the modules:
    - Just as with a standalone configuration, users will need to identify all of the modules being used for processing, in the correct order and correct paths.
- Define each module's variables:
    - All variables are still defined as previously mentioned in the `README-single-modules.md`. However, the only caveat with the multi-module implementation is aligning the input/source and output directories between modules.
        - **The output of a given module will always be the same path as the input to the following module.**
        - **All non-primary modules will have the same path for their source and output directories.**
- Follow the template:
    - Just as with a standalone configuration, users can specify module arguments within each module's template.

## General Comments
- The multimodule configuration will provide outputs at each stage of processing, which are used as input to subsequent stages. This design is meant to support easier troubleshooting between modules, if needed.
- As imagery is processed, output files will be appended with the name of the processing module. Therefore, imagery processed with Modules A, P, and O will have filenames ending in `MODA_MODP_MODO`. 
    - Module P also updates the middle of the filenames from `P1BS` for panchromatic imagery and `M1BS` for multispectral imagery to `S3XS` which is a Maxar naming standard and a naming convention upheld throughout the use of Legacy PIPE.
- For multi-module processing, combinations can **only** be made in these two orders:
    > Module A -> Module P -> Module O
    > Module P -> Module O
    - Atmospheric correction is **always** the first processing step if it is included, and imagery **must** be pansharpened before it is orthorectified.
- For more thorough discussions on variables and structure for individual modules, please browse the README for each:
    - [Module A](https://code.ornl.gov/GSHS/common/pipe/module-a/-/blob/main/README.md)
    - [Module P](https://code.ornl.gov/GSHS/common/pipe/module-p/-/blob/main/README.md)
    - [Module O](https://code.ornl.gov/GSHS/common/pipe/module-o/-/blob/main/README.md)