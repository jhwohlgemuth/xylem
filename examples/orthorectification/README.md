# Configuration for: Orthorectification

## Breaking Down the Configuration
- Specify the input directory or series of directories:
    - Use the `input` value to parse through imagery in parallel. 
        - In the associated `config.json`, multiple directories are listed. Note that when listing multiple directories, they are listed within `[ ]`, signaling to Xylem these specific directories should be concurrently processed.
        ```json
        "input": [
            "/path/to/output_MODP/order1",
            "/path/to/output_MODP/order2",
            "/path/to/output_MODP/order3",
            "/path/to/output_MODP/order4"
        ]
        ```
        - A single directory may also be specified. Xylem will parse through this main directory and concurrently process all available subdirectories.
        ```json
        "input": 
            "/path/to/output_MODP"
        ```
- Define the module:
    - Within the module variable, specify the `name` and `uri` to the module. The `uri` specifically informs Xylem from where to install the module environment. For example, if a user has Xylem installed and has cloned each of the modules in their `dev` directory within a Docker container, this path may look something like:
    ```json
        "uri": "file:///root/dev/module-o"
    ```
- Define the module variables:
    - Variables are unique to each module (detailed below).
- Simply follow the `template`:
    - This section is built from the module's `Makefile`. Here, users can specify arguments and associated variables. Below is an example template for Module O:
    ```json
        "template": {
                "command": "python",
                "environment": {
                    "name": "module-o",
                    "manager": "conda"
                },
                "arguments": [
                    "-m",
                    "lib",
                    "--source_directory",
                    "{{ INPUT }}",
                    "--output_directory",
                    "{{ OUTPUT_DIRECTORY }}"
                ]
            }
    ```

## Module Variables
For more thorough discussions on variables and overall structure, along with a link to technical documentation, please browse the README:
- [Module O](https://code-int.ornl.gov/gshs/common/imagery-processing/module-o/-/blob/main/README.md)

### Module O: ***O***rthorectification
- 'source_directory': the directory containing the output of Module P. Module O can be run as a unique module following the example `config.json`. HOWEVER, the input MUST have already been processed through Module P.
- 'output_directory': the directory in which to store the output of Module O processing. This argument should match the `source_directory` for Module O.

## Running the Module
- To run the standalone module independently of any workflow:
    ```bash
    make run
    ```
- To run the module with Xylem, at maximum verbosity, using the `config.json`:
    ```bash
    xylem run -vvvv
    ```