---
title: "PIPE: Parallel Integration Processing Engine"
tags:
  - Rust
  - JSON
  - automation
  - workflow
authors:
  - name: Jason Wohlgemuth
    orcid: 0000-0002-2057-9115
    corresponding: true
    affiliation: 1
  - name: Cecilia N. Clark
    orcid: 0000-0002-4826-2152
    affiliation: 1
affiliations:
  - name: Oak Ridge National Laboratory, Oak Ridge, Tennessee, TN, United States of America
    index: 1
    ror: 01qz5mb56
date: 
bibliography: paper.bib
---

# Summary
We present PIPE, a novel, portable workflow enabling fully automated and parallel processing of large-scale data. Originally 
designed to process satellite imagery for geospatial analytics, PIPE supports the repeatable execution of many processing tasks 
across the scientific domain. The PIPE configuration can codify almost any script project task: if a project can be built and 
run via a finite set of script commands without manual intervention, it can be configured as a standalone PIPE module.

# Statement of Need
Researchers who handle terabyte-scale datasets often rely on ad-hoc scripts bound to specific machines to process data, 
resulting in difficulties with high-performance processing, reproducibility, and peer review. Building a reliable and robust 
workflow is pertinent to the overall goals of processing voluminous and complex data. Developing a reliable pipeline provides 
data quality assurance by incorporating quality checks and validation stages to ensure outputs meet predefined accuracy and 
usability standards. Additionally, mechanisms to detect, report, and correct errors in the data or processing provides an 
additional layer of strength to the workflow. Overall, the community lacks a lightweight, stand-alone engine which brings 
high-level parallelism and strict reproducibility to any data domain without imposing new domain logic or infrastructure 
overhead. 

# Features and Implementation

## Overview
PIPE provides a portable, straightforward, trustworthy workflow considerate of performance concerns and limitations. 
Toward this goal, PIPE can be configured via flat files and executed in multiple cross-platform and cross-architecture 
contexts. In general terms, PIPE is a command line interface (CLI) application and a collection of standards and conventions. 
The CLI application was developed using the Rust programming language, providing unique benefits such as memory safety and 
“fearless” parallel programming which turns many concurrency errors into compile-time errors to be addressed before code 
execution.  

Simply put, researchers specify any number of processing modules in a plain-text, configuration file, which serves as the 
input to PIPE. Depending on the module specified, PIPE then pulls version-locked modules from a trusted repository, for 
reproducibility, or loads editable local copies, so researchers can tailor the processing logic without altering the 
underlying workflow. Finally, PIPE prepares and executes the specified modules in parallel, identifying any fatal errors 
or verification issues before execution.  

PIPE offers several advantages as a fully modular workflow through scalability, flexibility, maintainability, and 
reusability. Throughout the workflow development, we implemented methodologies and practices for sustainable scientific 
software research and development to be implemented at scale with emphasis on automation, portability, and correctness. 
Specifically, the PIPE workflow benefits the research community by: 
- Providing a modular, portable platform optimized for processing large-scale data 
- Detecting fatal issues before execution, virtually eliminating non-productive runtime 
- Constraining complexity to processing modules, which are resolved dynamically 
- Running as a single executable available on multiple operating systems and architectures 

# Acknowledgements
This manuscript has been authored by UT-Battelle, LLC under Contract No.DE-AC05-00OR22725 with the U.S. Department of Energy. 
The United States Government retains and the publisher, by accepting the article for publication, acknowledges that the United 
States Government retains a non-exclusive, paidup, irrevocable, world-wide license to publish or reproduce the published form 
of this manuscript, or allow others to do so, for United States Government purposes. The Department of Energy will provide 
public access to these results of federally sponsored research in accordance with the DOE Public Access Plan.

# References
See [paper.bib](https://code.ornl.gov/GSHS/common/pipe/command/-/blob/paper/paper.bib?ref_type=heads)